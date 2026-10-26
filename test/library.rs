use sxpb::{
    Dict, Field, List, Lone, MAX_DEPTH, Many, ManyofEntry as Entry, Mesg, Nest, NestItem, Number,
    NumberKind, Value, from_str, to_string,
};

fn parse(s: &str) -> Value {
    from_str(s).unwrap_or_else(|e| panic!("{s:?}: {e}"))
}
fn field_value(s: &str) -> Value {
    parse(s).get("a").unwrap().clone()
}
fn roundtrip(s: &str) -> Value {
    let value = parse(s);
    let printed = to_string(&value).unwrap();
    assert_eq!(parse(&printed), value, "printed {printed:?}");
    assert_eq!(to_string(&parse(&printed)).unwrap(), printed);
    value
}
fn invalid(cases: &[&str]) {
    for source in cases {
        assert!(from_str(source).is_err(), "accepted {source:?}");
    }
}
fn message(value: Value) -> Value {
    Value::Mesg(vec![Field::new("a", value)].into())
}

#[test]
fn message_order_and_dict_kind() {
    let value = roundtrip("(z 1)(a ()(nested +true))(m)");
    let fields = value.as_fields().unwrap();
    assert_eq!(
        fields.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
        ["z", "a", "m"]
    );
    assert!(matches!(value.get("a"), Some(Value::Dict(_))));
    assert!(matches!(value.get("m"), Some(Value::Mesg(f)) if f.is_empty()));
    assert_eq!(parse(""), Value::Mesg(vec![].into()));
    assert_eq!(parse("; comment\n"), Value::Mesg(vec![].into()));
    assert_eq!(roundtrip("()"), Value::Dict(vec![].into()));
    assert_ne!(parse("(a ())"), parse("(a)"));
}

#[test]
fn canonical_simple_message_output() {
    for source in [
        "(\"quoted name\" value)",
        "(answer 42)\n(enabled +true)\n(label paprika)",
    ] {
        assert_eq!(to_string(&roundtrip(source)).unwrap(), source);
    }
}

#[test]
fn document_items_use_lines_and_nested_bodies_stay_compact() {
    for (source, expected) in [
        (
            "(first 1) (second (x 2) (y 3))",
            "(first 1)\n(second (x 2) (y 3))",
        ),
        ("() (first 1) (second 2)", "()\n(first 1)\n(second 2)"),
        ("(()) 1 2", "(())\n1\n2"),
        ("(()) (x 1) 2", "(())\n(x 1)\n2"),
        ("(\"\") leaf twig", "(\"\")\nleaf\ntwig"),
    ] {
        assert_eq!(to_string(&roundtrip(source)).unwrap(), expected);
        assert!(!expected.ends_with('\n'));
    }
}

#[test]
fn unambiguous_string_atoms_and_names_print_bare() {
    for text in [
        "word",
        "-",
        ".",
        "/",
        "--",
        "..",
        "-name",
        ".name",
        "é",
        "日本語",
        "back\\slash",
    ] {
        let value: Value = Mesg::from(vec![Field::new(text, text)]).into();
        let printed = to_string(&value).unwrap();
        assert_eq!(printed, format!("({text} {text})"));
        assert_eq!(parse(&printed), value);
        assert_eq!(to_string(&parse(&printed)).unwrap(), printed);
    }
}

#[test]
fn ambiguous_or_delimited_string_atoms_and_names_remain_quoted() {
    for text in [
        "",
        "+",
        "+.",
        "-1",
        ".5",
        "-.",
        ".+",
        "+true",
        "+false",
        "50mm",
        "1e2",
        "-0.0",
        "two words",
        " leading",
        "trailing ",
        "two  spaces",
        "x(y)",
        "x;y",
        "x\"y",
        "x\ny",
        "x\ty",
        "x\ry",
        "\0",
        "\u{8}",
        "\u{7f}",
        "\u{85}",
        "\u{2028}",
        "\u{2029}",
        "\u{feff}x",
        "x\u{feff}y",
    ] {
        let value = message(Value::from(text));
        let printed = to_string(&value).unwrap();
        assert!(printed.starts_with("(a \""), "{text:?}: {printed:?}");
        assert_eq!(parse(&printed), value, "{text:?}");
        assert_eq!(to_string(&parse(&printed)).unwrap(), printed);
        if !text.is_empty() {
            let named: Value = Mesg::from(vec![Field::new(text, 1)]).into();
            let printed = to_string(&named).unwrap();
            assert!(printed.starts_with("(\""), "{text:?}: {printed:?}");
            assert_eq!(parse(&printed), named);
        }
    }
}

#[test]
fn canonical_empty_nest_items_preserve_discriminators() {
    assert_eq!(
        to_string(&roundtrip("(\"\") \"\"")).unwrap(),
        "(\"\")\n\"\""
    );
    assert_eq!(
        to_string(&roundtrip("(\"\") (\"\" (\"\"))")).unwrap(),
        "(\"\")\n(\"\" (\"\"))"
    );
}

#[test]
fn standalone_container_types_move_without_allocating_and_expose_typed_access() {
    macro_rules! check_sequence {
        ($ty:ident, $first:expr, $second:expr, $get:ident, $get_mut:ident) => {{
            let first = $first;
            let second = $second;
            let items = vec![first.clone()];
            let pointer = items.as_ptr();
            let mut container = $ty::from(items);
            assert_eq!(container.as_slice().as_ptr(), pointer);
            assert_eq!(container.len(), 1);
            assert!(!container.is_empty());
            assert!($ty::new().is_empty());
            assert!($ty::default().is_empty());
            container.push(second.clone());
            assert_eq!(container.iter().count(), 2);
            for item in &mut container {
                *item = first.clone();
            }
            container
                .iter_mut()
                .next()
                .map(|item| *item = second.clone());
            container.as_mut_slice()[1] = second.clone();
            container.extend([first.clone()]);
            let collected: $ty = (&container).into_iter().cloned().collect();
            assert_eq!(collected, container);
            assert_eq!(
                collected.clone().into_iter().collect::<Vec<_>>(),
                collected.clone().into_vec()
            );
            let mut value: Value = container.clone().into();
            assert_eq!(value.$get(), Some(&container));
            value.$get_mut().unwrap().push(second);
            assert_eq!(value.$get().unwrap().len(), container.len() + 1);
            let pointer = container.as_slice().as_ptr();
            let moved: Value = container.into();
            assert_eq!(moved.$get().unwrap().as_slice().as_ptr(), pointer);
        }};
    }
    check_sequence!(
        Mesg,
        Field::new("a", 1),
        Field::new("b", 2),
        as_mesg,
        as_mesg_mut
    );
    check_sequence!(
        Dict,
        Field::new("a", 1),
        Field::new("b", 2),
        as_dict,
        as_dict_mut
    );
    check_sequence!(List, Value::from(1), Value::from(2), as_list, as_list_mut);
    check_sequence!(
        Many,
        Entry::Anonymous(1.into()),
        Entry::Anonymous(2.into()),
        as_many,
        as_many_mut
    );
    check_sequence!(
        Nest,
        NestItem::Leaf("a".into()),
        NestItem::Leaf("b".into()),
        as_nest,
        as_nest_mut
    );
}

#[test]
fn typed_messages_and_dicts_remain_distinct() {
    let mut mesg = Mesg::from(vec![Field::new("x", 1)]);
    let mut dict = Dict::from(vec![Field::new("x", 1)]);
    assert_eq!(mesg.get("x"), dict.get("x"));
    *mesg.get_mut("x").unwrap() = 2.into();
    *dict.get_mut("x").unwrap() = 3.into();
    let message: Value = mesg.into();
    let dictionary: Value = dict.into();
    assert!(message.as_dict().is_none());
    assert!(dictionary.as_mesg().is_none());
    assert_eq!(parse(&to_string(&message).unwrap()), message);
    assert_eq!(parse(&to_string(&dictionary).unwrap()), dictionary);
}

#[test]
fn typed_lone_and_nest_subtrees() {
    let lone = Lone::new("choice", 7);
    let mut value: Value = lone.clone().into();
    assert_eq!(value.as_lone(), Some(&lone));
    value.as_lone_mut().unwrap().name = "other".into();
    let wrapped = message(value);
    assert_eq!(parse(&to_string(&wrapped).unwrap()), wrapped);

    fn inspect(nest: &sxpb::Nest) -> usize {
        nest.len()
    }
    let mut tree = Nest::new();
    tree.push(NestItem::Branch {
        name: Some("branch".into()),
        children: Nest::from(vec![NestItem::Leaf("leaf".into())]),
    });
    let value: Value = tree.into();
    assert_eq!(inspect(value.as_nest().unwrap()), 1);
    let NestItem::Branch { children, .. } = &value.as_nest().unwrap().as_slice()[0] else {
        panic!()
    };
    assert_eq!(inspect(children), 1);
    assert_eq!(parse(&to_string(&value).unwrap()), value);
}

#[test]
fn number_grammar_and_exact_storage() {
    for s in [
        "0",
        "-0",
        "+0001",
        "999999999999999999999999999999999999999999999999999999",
        ".5",
        "-.5",
        "+.5",
        "1.",
        "1.e-2",
        "1E999999999999",
        "-0.0",
    ] {
        let n: Number = s.parse().unwrap();
        assert_eq!(n.as_str(), s);
        assert_eq!(
            roundtrip(&format!("(a {s})")).get("a").unwrap().as_number(),
            Some(&n)
        );
    }
    for s in [
        "", "+", "-", ".", "-.", "+.", "1e", "1e+", "1_000", " 1", "1 ", "NaN", "inf", "١",
    ] {
        assert!(s.parse::<Number>().is_err(), "accepted {s:?}");
    }
    assert_eq!("1".parse::<Number>().unwrap().kind(), NumberKind::Integer);
    assert_eq!("1e0".parse::<Number>().unwrap().kind(), NumberKind::Float);
    assert_ne!("1".parse::<Number>().unwrap(), "+1".parse().unwrap());
    assert_eq!(Number::from(i128::MAX).to_i128().unwrap(), i128::MAX);
    assert_eq!(Number::from(u128::MAX).to_u128().unwrap(), u128::MAX);
    assert!("1.0".parse::<Number>().unwrap().to_i128().is_err());
    assert!(
        "999999999999999999999999999999999999999999"
            .parse::<Number>()
            .unwrap()
            .to_i128()
            .is_err()
    );
    assert!(
        "1e9999"
            .parse::<Number>()
            .unwrap()
            .to_f64()
            .unwrap()
            .is_infinite()
    );
}

#[test]
fn float_construction_keeps_float_kind() {
    for f in [0.0, -0.0, 1.0, 1e100, 1e-100, f64::MIN_POSITIVE, f64::MAX] {
        let n = Number::try_from(f).unwrap();
        assert_eq!(n.kind(), NumberKind::Float);
        assert_eq!(n.to_f64().unwrap().to_bits(), f.to_bits());
        roundtrip(&format!("(a {n})"));
    }
    for f in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(Number::try_from(f).is_err());
    }
}

#[test]
fn strings_join_only_unquoted_neighbors() {
    for (source, expected) in [
        ("(a one two)", "one two"),
        (r#"(a "one" two)"#, "onetwo"),
        (r#"(a one "two" three four)"#, "onetwothree four"),
        (r#"(a "" +true 01 -.)"#, "+true 01 -."),
        (r#"(a "")"#, ""),
        (
            r#"(a """
  line
""")"#,
            "\n  line\n",
        ),
        (r#"(a "x;y()")"#, "x;y()"),
    ] {
        assert_eq!(
            roundtrip(source).get("a").and_then(Value::as_str),
            Some(expected)
        );
    }
    invalid(&[
        "(a 1 2)",
        "(a +true x)",
        "(a +truex)",
        "(a -.)",
        "(a .+)",
        "(a 50mm)",
    ]);
    for atom in ["-", ".", "/", "--", "..", "-name", ".name"] {
        roundtrip(&format!("(a {atom})"));
    }
}

#[test]
fn basic_escape_profile() {
    let data = roundtrip(r#"(a "\"\\\t\n\v\f\r")"#);
    assert_eq!(
        data.get("a").and_then(Value::as_str),
        Some("\"\\\t\n\u{b}\u{c}\r")
    );
    for escape in [r"\u0041", r"\b", r"\/", r"\q", r"\0", r"\e", "\\ "] {
        invalid(&[
            &format!("(a \"{escape}\")"),
            &format!("(a \"\"\"{escape}\"\"\")"),
        ]);
    }
    assert_eq!(field_value("(a \"x\ry\")"), Value::from("xy"));
    assert_eq!(field_value("(a \"x\\\ny\")"), Value::from("xy"));
    assert_eq!(field_value("(a \"x\\\r\ny\")"), Value::from("xy"));
    assert_eq!(field_value("(a \"\"\"x\\\ny\"\"\")"), Value::from("xy"));
}

#[test]
fn truncated_unicode_strings_return_syntax_errors() {
    for source in [r#"(a "é🦊\"tail")"#, "(a \"\"\"é🦊\\\r\ntail\"\"\")"] {
        roundtrip(source);
        // Every nonempty proper UTF-8 prefix is missing the field's closing
        // parenthesis (and often a string delimiter or escape operand too).
        for (end, _) in source.char_indices().skip(1) {
            let prefix = &source[..end];
            let error = from_str(prefix).unwrap_err();
            assert!(prefix.is_char_boundary(error.offset), "{error}");
        }
    }
}

#[test]
fn all_control_characters_and_unicode_roundtrip() {
    let text: String = (0..=0x7f)
        .map(|c| char::from_u32(c).unwrap())
        .chain(" 🕵️‍♀️日本語é\u{feff}".chars())
        .collect();
    let value = message(Value::from(text));
    assert_eq!(parse(&to_string(&value).unwrap()), value);
    roundtrip("(é hello\u{feff}world)");
    // Non-ASCII whitespace is atom content, not a separator.
    assert_eq!(
        roundtrip("(a\u{a0}1)").as_fields().unwrap()[0].name,
        "a\u{a0}1"
    );
    invalid(&[
        "\u{feff}(a 1)",
        "(a 1) \u{feff}",
        "(a 1\u{a0}2)",
        "(a \u{feff}x)",
    ]);
}

#[test]
fn discriminators_whitespace_and_comments() {
    roundtrip("(a ( ( ) ) 1 2) (b ( \"\" ) leaf) (c ( ) (x 1))");
    roundtrip(";before\n(a 1);after\n(b 2)");
    invalid(&[
        "(a (;c\n()) 1)",
        "(a (()) (()) 1)",
        "(a ((()) 1)",
        "(a (\"\") (\"\") x)",
    ]);
}

#[test]
fn arrays_use_first_kind_and_preserve_spellings() {
    let value = roundtrip("(a (()) word 02 +03 4.0 +true -.)");
    assert_eq!(
        value.get("a").unwrap().as_array().unwrap(),
        ["word", "02", "+03", "4.0", "+true", "-."].map(Value::from)
    );
    let value = roundtrip("(a (()) +true 00 +01 +false)");
    assert_eq!(
        value.get("a").unwrap().as_array().unwrap(),
        [true, false, true, false].map(Value::from)
    );
    roundtrip("(a (()) 1 2.0 3e0)");
    roundtrip("(a (()) () (() (x 1)))");
    roundtrip(r#"(a (()) "" ("" "x" y))"#);
    invalid(&[
        "(a (()) 1 x)",
        "(a (()) +true -0)",
        "(a (()) +false 1.0)",
        "(a (()) +true 2)",
        "(a (()) () 1)",
        "(a (()) 1 ())",
        "(a (()) (() (x 1)) (x 2))",
        "(a (()) -. x)",
    ]);
}

#[test]
fn manyof_identity_and_first_anonymous_kind() {
    let value = roundtrip("((a) (value 1) 2 (value 3) 4)");
    let Value::Many(items) = value.get("a").unwrap() else {
        panic!()
    };
    assert!(matches!(&items.as_slice()[0], Entry::Named(f) if f.name == "value"));
    assert!(matches!(
        &items.as_slice()[1],
        Entry::Anonymous(Value::Number(_))
    ));
    roundtrip("(a (()) (x 1) one (middle +false) 02 +true)");
    roundtrip("((a) () (x 1) (() (y 2)))");
    roundtrip("((a))");
    roundtrip("((a) \"\" x)");
    invalid(&[
        "((a) (x 1) 2 +true)",
        "(a (()) () (x 1))",
        "((a) (x 1) -. )",
    ]);
}

#[test]
fn loneof_all_payloads() {
    for source in [
        "((a choice))",
        "((a choice) 1)",
        "((a choice) ())",
        "((a choice) (x 1))",
        "((a choice) (()) 1 2)",
        "((a choice) (()) (x 1) 2)",
        "((a choice) (\"\") leaf)",
    ] {
        roundtrip(source);
    }
    invalid(&["((a b c) 1)", "((a \"\") 1)"]);
}

#[test]
fn append_resolves_existing_paths_and_inherits_kind() {
    let result = roundtrip(
        "(m () (a (()) word)) ((+. m a) (()) 02 +true) (b (()) +false) ((+. b) (()) +01 00)",
    );
    assert_eq!(
        result
            .get("m")
            .unwrap()
            .get("a")
            .unwrap()
            .as_array()
            .unwrap(),
        ["word", "02", "+true"].map(Value::from)
    );
    roundtrip("(a (())) ((+. a) (()) 1 2) ((+. a) (()) 3)");
    roundtrip("((a) (x 1)) ((+. a) (()) (x 2) one 02)");
    roundtrip("((a)) ((+. a) (()) () (x 2) (() (y 1)))");
    roundtrip("(a (()) (() (b (())) ((+. b) (()) 1)))");
    roundtrip(r#"("long key" (a (()) 1)) ((+. "long key" a) (()) 2)"#);
    invalid(&[
        "(a (())) ((+. a) 1)",
        "((+. a) (()) 1)",
        "(a 1) ((+. a) (()) 2)",
        "(a (())) ((+.) (()) 1)",
        "(a (()) 1) ((+. a) (()) word)",
        "(a (())) ((+. a) (()) (x 1))",
        "((a) 1) ((+. a) (()) +true)",
        "((a) (x 1)) ((+. a x) (()) 1)",
    ]);
}

#[test]
fn duplicates_and_trailing_input_reject() {
    invalid(&[
        "(a 1)(a 2)",
        "(a (()))(a (()))",
        "(a)(a)",
        "() ()",
        "(a (b 1) ())",
        "(a 1))",
        "(a 1) junk",
        "(a 1) (())",
        "(a",
        "(",
        ")",
        "((\"\") x)",
        "(\"\" x)",
        "(+. x)",
    ]);
}

#[test]
fn nests_preserve_strings_labels_and_anonymous_branches() {
    roundtrip(
        r#"(a ("") +true +truex -. "" word (7 x) (-0.5 x) ("" "") ("" ("")) (("") x) (named "" hello world))"#,
    );
    assert_eq!(
        field_value(r#"(a ("") "" word)"#),
        Value::Nest(vec![NestItem::Leaf("".into()), NestItem::Leaf("word".into())].into())
    );
    invalid(&[
        r#"(a ("") (("")))"#,
        r#"(a ("") x ("") y)"#,
        r#"(a ("") (+0.5 x))"#,
        r#"(a ("") (x (()) 1))"#,
    ]);
}

#[test]
fn root_kinds_and_unrepresentable_manyofs() {
    for source in [
        "(())",
        "(()) 1 2",
        "(()) (x 1) 2",
        "(\"\")",
        "(\"\") leaf",
        "() (a 1)",
    ] {
        roundtrip(source);
    }
    for value in [
        Value::Many(vec![].into()),
        Value::Many(vec![Entry::Anonymous(1.into())].into()),
        Value::from("x"),
        Value::from(1),
        Value::from(true),
        Value::Lone(Lone {
            name: "x".into(),
            value: Box::new(1.into()),
        }),
    ] {
        assert!(to_string(&value).is_err(), "accepted {value:?}");
    }
}

#[test]
fn serializer_rejects_invalid_caller_trees() {
    let bad = [
        Value::Mesg(vec![Field::new("x", 1), Field::new("x", 2)].into()),
        Value::Dict(vec![Field::new("", 1)].into()),
        message(Value::List(vec![1.into(), "x".into()].into())),
        message(Value::List(vec![true.into(), 1.into()].into())),
        message(Value::List(vec!["x".into(), 1.into()].into())),
        message(Value::List(vec![Value::Dict(vec![].into())].into())),
        message(Value::Many(
            vec![Entry::Anonymous(Value::List(vec![].into()))].into(),
        )),
        message(Value::Many(
            vec![Entry::Anonymous(true.into()), Entry::Anonymous(1.into())].into(),
        )),
        message(Value::Lone(Lone {
            name: "".into(),
            value: Box::new(1.into()),
        })),
        message(Value::Lone(Lone {
            name: "x".into(),
            value: Box::new(Value::Many(vec![].into())),
        })),
        Value::Nest(
            vec![NestItem::Branch {
                name: Some("".into()),
                children: vec![].into(),
            }]
            .into(),
        ),
    ];
    for value in bad {
        assert!(to_string(&value).is_err(), "accepted {value:?}");
    }
}

#[test]
fn error_offsets_are_utf8_and_columns_are_characters() {
    let source = "(a é)\n(b 1) )";
    let error = from_str(source).unwrap_err();
    assert_eq!(error.offset, source.rfind(')').unwrap());
    assert_eq!((error.line, error.column), (2, 7));
    let error = from_str("(a é +true) )").unwrap_err();
    assert_eq!(error.column, 13);
    assert!(std::error::Error::source(&error).is_none());
    assert!(error.to_string().contains("byte"));
}

#[test]
fn depth_limits_parse_and_serialize() {
    let source = format!("{}1{}", "(a ".repeat(MAX_DEPTH), ")".repeat(MAX_DEPTH));
    let value = roundtrip(&source);
    assert_eq!(value.clone(), value);
    let deeper = format!("(a {source})");
    assert!(from_str(&deeper).unwrap_err().message.contains("depth"));
    let mut value = Value::from(1);
    for _ in 0..=MAX_DEPTH {
        value = message(value);
    }
    assert!(to_string(&value).unwrap_err().message.contains("depth"));
}

fn append_depth_source(rounds: usize) -> String {
    let mut source = String::from("(leaf 1)");
    for _ in 0..rounds {
        source = format!(
            "{}(a (())){} ((+. {}a) (()) (() {}))",
            "(m ".repeat(128),
            ")".repeat(128),
            "m ".repeat(128),
            source,
        );
    }
    source
}

#[test]
fn append_composition_cannot_bypass_tree_depth_limit() {
    for rounds in [4, 60] {
        let source = append_depth_source(rounds);
        let error = std::thread::spawn(move || from_str(&source).unwrap_err())
            .join()
            .unwrap();
        assert!(error.message.contains("depth"), "{error}");
    }
}

fn generated_value(state: &mut u64, depth: usize) -> Value {
    *state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
    let choice = (*state >> 32) as usize;
    let kind = choice % if depth == 0 { 3 } else { 9 };
    match kind {
        0 => Value::from(
            [
                "",
                "hello world",
                "()\\\";",
                "é\0\u{feff}\r\n",
                "+true",
                "1e9999",
            ][choice % 6],
        ),
        1 => Value::Bool(choice % 2 == 0),
        2 => Value::Number(
            [
                "0",
                "-0.0",
                "+0001",
                "1e9999",
                "-.5",
                "1234567890123456789012345678901234567890",
            ][choice % 6]
                .parse()
                .unwrap(),
        ),
        3 => Value::Mesg(
            vec![
                Field::new("first", generated_value(state, depth - 1)),
                Field::new("second", generated_value(state, depth - 1)),
            ]
            .into(),
        ),
        4 => Value::Dict(vec![Field::new("dict key", generated_value(state, depth - 1))].into()),
        5 => Value::List(
            vec![
                Value::Mesg(vec![Field::new("child", generated_value(state, depth - 1))].into()),
                Value::Mesg(vec![].into()),
            ]
            .into(),
        ),
        6 => Value::Lone(Lone {
            name: "choice".into(),
            value: Box::new(Value::Mesg(
                vec![Field::new("payload", generated_value(state, depth - 1))].into(),
            )),
        }),
        7 => Value::Many(
            vec![
                Entry::Named(Field::new("value", generated_value(state, depth - 1))),
                Entry::Anonymous(true.into()),
                Entry::Named(Field::new("value", 1)),
                Entry::Anonymous(false.into()),
            ]
            .into(),
        ),
        _ => Value::Nest(
            vec![
                NestItem::Leaf("".into()),
                NestItem::Branch {
                    name: None,
                    children: vec![].into(),
                },
                NestItem::Branch {
                    name: Some("-0.5".into()),
                    children: vec![NestItem::Leaf("two words".into())].into(),
                },
            ]
            .into(),
        ),
    }
}

#[test]
fn generated_caller_trees_roundtrip() {
    let mut state = 0x1234_5678_u64;
    for _ in 0..512 {
        let value = message(generated_value(&mut state, 5));
        let text = to_string(&value).unwrap();
        assert_eq!(parse(&text), value, "output {text:?}");
        assert_eq!(to_string(&parse(&text)).unwrap(), text);
    }
}

#[test]
fn deterministic_adversarial_inputs_do_not_panic() {
    let alphabet = [
        "(", ")", "\"", "\\", ";", "\n", "", "-. ", "+true ", "é ", "\0", "(())", "(\"\")",
    ];
    let mut state = 0x55aa_1234_u64;
    for _ in 0..3000 {
        let mut source = String::new();
        for _ in 0..24 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            source.push_str(alphabet[(state >> 32) as usize % alphabet.len()]);
        }
        if let Ok(value) = from_str(&source) {
            if let Ok(printed) = to_string(&value) {
                assert_eq!(parse(&printed), value, "input {source:?}");
            }
        }
    }
}
