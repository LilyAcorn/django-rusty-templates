from textwrap import dedent

import pytest
from django.template import engines
from django.template.exceptions import TemplateSyntaxError


def test_render_for_loop():
    template = "{% for x in y %}{{ x }}{% endfor %}"
    django_template = engines["django"].from_string(template)
    rust_template = engines["rusty"].from_string(template)

    y = [1, 2, "foo"]
    expected = "12foo"
    assert django_template.render({"y": y}) == expected
    assert rust_template.render({"y": y}) == expected


def test_render_for_loop_reversed():
    template = "{% for x in y reversed %}{{ x }}{% endfor %}"
    django_template = engines["django"].from_string(template)
    rust_template = engines["rusty"].from_string(template)

    y = [1, 2, "foo"]
    expected = "foo21"
    assert django_template.render({"y": y}) == expected
    assert rust_template.render({"y": y}) == expected


def test_render_for_loop_filter():
    template = "{% for x in y|upper %}{{ x }}{% endfor %}"
    django_template = engines["django"].from_string(template)
    rust_template = engines["rusty"].from_string(template)

    y = "foo"
    expected = "FOO"
    assert django_template.render({"y": y}) == expected
    assert rust_template.render({"y": y}) == expected


def test_render_for_loop_filter_reversed():
    template = "{% for x in y|upper reversed %}{{ x }}{% endfor %}"
    django_template = engines["django"].from_string(template)
    rust_template = engines["rusty"].from_string(template)

    y = "foo"
    expected = "OOF"
    assert django_template.render({"y": y}) == expected
    assert rust_template.render({"y": y}) == expected


def test_render_for_loop_unpack_tuple():
    template = "{% for x, y, z in l %}{{ x }}-{{ y }}-{{ z }}\n{% endfor %}"
    django_template = engines["django"].from_string(template)
    rust_template = engines["rusty"].from_string(template)

    l = [(1, 2, 3), ("foo", "bar", "spam")]
    expected = "1-2-3\nfoo-bar-spam\n"
    assert django_template.render({"l": l}) == expected
    assert rust_template.render({"l": l}) == expected


def test_render_for_loop_unpack_tuple_no_whitespace():
    template = "{% for x,y in l %}{{ x }}-{{ y }}\n{% endfor %}"
    django_template = engines["django"].from_string(template)
    rust_template = engines["rusty"].from_string(template)

    l = [(1, 2), ("foo", "bar")]
    expected = "1-2\nfoo-bar\n"
    assert django_template.render({"l": l}) == expected
    assert rust_template.render({"l": l}) == expected


def test_render_for_loop_unpack_dict_items():
    template = "{% for x, y in d.items %}{{ x }}: {{ y }}\n{% endfor %}"
    django_template = engines["django"].from_string(template)
    rust_template = engines["rusty"].from_string(template)

    d = {"foo": 1, "bar": 2}
    expected = "foo: 1\nbar: 2\n"
    assert django_template.render({"d": d}) == expected
    assert rust_template.render({"d": d}) == expected


def test_render_for_loop_counter():
    template = "{% for x in y %}{{ x }}: {{ forloop.counter }}\n{% endfor %}"
    django_template = engines["django"].from_string(template)
    rust_template = engines["rusty"].from_string(template)

    y = ["foo", "bar", "spam"]
    expected = "foo: 1\nbar: 2\nspam: 3\n"
    assert django_template.render({"y": y}) == expected
    assert rust_template.render({"y": y}) == expected


def test_render_for_loop_counter0():
    template = "{% for x in y %}{{ x }}: {{ forloop.counter0 }}\n{% endfor %}"
    django_template = engines["django"].from_string(template)
    rust_template = engines["rusty"].from_string(template)

    y = ["foo", "bar", "spam"]
    expected = "foo: 0\nbar: 1\nspam: 2\n"
    assert django_template.render({"y": y}) == expected
    assert rust_template.render({"y": y}) == expected


def test_render_for_loop_revcounter():
    template = "{% for x in y %}{{ x }}: {{ forloop.revcounter }}\n{% endfor %}"
    django_template = engines["django"].from_string(template)
    rust_template = engines["rusty"].from_string(template)

    y = ["foo", "bar", "spam"]
    expected = "foo: 3\nbar: 2\nspam: 1\n"
    assert django_template.render({"y": y}) == expected
    assert rust_template.render({"y": y}) == expected


def test_render_for_loop_revcounter0():
    template = "{% for x in y %}{{ x }}: {{ forloop.revcounter0 }}\n{% endfor %}"
    django_template = engines["django"].from_string(template)
    rust_template = engines["rusty"].from_string(template)

    y = ["foo", "bar", "spam"]
    expected = "foo: 2\nbar: 1\nspam: 0\n"
    assert django_template.render({"y": y}) == expected
    assert rust_template.render({"y": y}) == expected


def test_render_for_loop_first():
    template = "{% for x in y %}{{ x }}: {{ forloop.first }}\n{% endfor %}"
    django_template = engines["django"].from_string(template)
    rust_template = engines["rusty"].from_string(template)

    y = ["foo", "bar", "spam"]
    expected = "foo: True\nbar: False\nspam: False\n"
    assert django_template.render({"y": y}) == expected
    assert rust_template.render({"y": y}) == expected


def test_render_for_loop_last():
    template = "{% for x in y %}{{ x }}: {{ forloop.last }}\n{% endfor %}"
    django_template = engines["django"].from_string(template)
    rust_template = engines["rusty"].from_string(template)

    y = ["foo", "bar", "spam"]
    expected = "foo: False\nbar: False\nspam: True\n"
    assert django_template.render({"y": y}) == expected
    assert rust_template.render({"y": y}) == expected


def test_render_for_loop_parentloop():
    template = """
    {% for x in xs %}
        {{ forloop.counter }}: {{ x }}
        {% for y in ys %}
            {{ forloop.parentloop.counter }}, {{ forloop.counter }}: {{ x }}, {{ y }}
        {% endfor %}
    {% endfor %}
    """
    django_template = engines["django"].from_string(template)
    rust_template = engines["rusty"].from_string(template)

    xs = ["x1", "x2", "x3"]
    ys = ["y1", "y2"]
    expected = """\
        1: x1
            1, 1: x1, y1
            1, 2: x1, y2
        2: x2
            2, 1: x2, y1
            2, 2: x2, y2
        3: x3
            3, 1: x3, y1
            3, 2: x3, y2"""

    def strip_whitespace_lines(s):
        lines = []
        for line in s.split("\n"):
            if line.strip():
                lines.append(line)
        return "\n".join(lines)

    assert (
        strip_whitespace_lines(django_template.render({"xs": xs, "ys": ys})) == expected
    )
    assert (
        strip_whitespace_lines(rust_template.render({"xs": xs, "ys": ys})) == expected
    )


def test_render_for_loop_empty():
    template = dedent("""
    <ul>
    {% for athlete in athlete_list %}
        <li>{{ athlete.name }}</li>
    {% empty %}
        <li>sorry, no athletes in this list.</li>
    {% endfor %}
    </ul>
    """)
    django_template = engines["django"].from_string(template)
    rust_template = engines["rusty"].from_string(template)

    expected = dedent("""
    <ul>

        <li>sorry, no athletes in this list.</li>

    </ul>
    """)
    assert django_template.render({}) == expected
    assert rust_template.render({}) == expected


def test_render_in_in_in():
    template = "{% for in in in %}{{ in }}\n{% endfor %}"
    django_template = engines["django"].from_string(template)
    rust_template = engines["rusty"].from_string(template)

    l = (1, 2, 3)
    expected = "1\n2\n3\n"
    assert django_template.render({"in": l}) == expected
    assert rust_template.render({"in": l}) == expected


def test_render_number_in_expression():
    template = "{% for 1 in l %}{{ 1 }}\n{% endfor %}"
    django_template = engines["django"].from_string(template)
    rust_template = engines["rusty"].from_string(template)

    l = (1, 2, 3)
    expected = "1\n1\n1\n"
    assert django_template.render({"l": l}) == expected
    assert rust_template.render({"l": l}) == expected


def test_missing_variable_no_in():
    template = "{% for %}{% endfor %}"

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["django"].from_string(template)

    assert (
        str(exc_info.value) == "'for' statements should have at least four words: for"
    )

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["rusty"].from_string(template)

    expected = """\
  × Expected at least one variable name in for loop:
   ╭────
 1 │ {% for %}{% endfor %}
   · ────┬────
   ·     ╰── in this tag
   ╰────
"""
    assert str(exc_info.value) == expected


def test_missing_variable_before_in():
    template = "{% for in %}{% endfor %}"

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["django"].from_string(template)

    assert (
        str(exc_info.value)
        == "'for' statements should have at least four words: for in"
    )

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["rusty"].from_string(template)

    expected = """\
  × Expected a variable name before the 'in' keyword:
   ╭────
 1 │ {% for in %}{% endfor %}
   ·        ─┬
   ·         ╰── before this keyword
   ╰────
"""
    assert str(exc_info.value) == expected


def test_missing_variable_before_in_four_words():
    template = "{% for in xs reversed %}{% endfor %}"

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["django"].from_string(template)

    assert (
        str(exc_info.value)
        == "'for' tag received an invalid argument: for in xs reversed"
    )

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["rusty"].from_string(template)

    expected = """\
  × Expected a variable name before the 'in' keyword:
   ╭────
 1 │ {% for in xs reversed %}{% endfor %}
   ·        ─┬
   ·         ╰── before this keyword
   ╰────
"""
    assert str(exc_info.value) == expected


def test_missing_in():
    template = "{% for x %}{% endfor %}"

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["django"].from_string(template)

    assert (
        str(exc_info.value) == "'for' statements should have at least four words: for x"
    )

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["rusty"].from_string(template)

    expected = """\
  × Expected the 'in' keyword or a variable name:
   ╭────
 1 │ {% for x %}{% endfor %}
   ·        ┬
   ·        ╰── after this name
   ╰────
"""
    assert str(exc_info.value) == expected


def test_missing_expression_after_in():
    template = "{% for x in %}{% endfor %}"

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["django"].from_string(template)

    assert (
        str(exc_info.value)
        == "'for' statements should have at least four words: for x in"
    )

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["rusty"].from_string(template)

    expected = """\
  × Expected an expression after the 'in' keyword:
   ╭────
 1 │ {% for x in %}{% endfor %}
   ·          ─┬
   ·           ╰── after this keyword
   ╰────
"""
    assert str(exc_info.value) == expected


def test_missing_expression_after_in_four_words():
    template = "{% for x, z in %}{% endfor %}"

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["django"].from_string(template)

    assert (
        str(exc_info.value)
        == "'for' statements should use the format 'for x in y': for x, z in"
    )

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["rusty"].from_string(template)

    expected = """\
  × Expected an expression after the 'in' keyword:
   ╭────
 1 │ {% for x, z in %}{% endfor %}
   ·             ─┬
   ·              ╰── after this keyword
   ╰────
"""
    assert str(exc_info.value) == expected


def test_unpack_1_tuple():
    template = "{% for x, in l %}{% endfor %}"

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["django"].from_string(template)

    assert str(exc_info.value) == "'for' tag received an invalid argument: for x, in l"

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["rusty"].from_string(template)

    expected = """\
  × Expected another variable when unpacking in for loop:
   ╭────
 1 │ {% for x, in l %}{% endfor %}
   ·        ┬
   ·        ╰── after this variable
   ╰────
"""
    assert str(exc_info.value) == expected


def test_invalid_variable_in_unpack():
    template = "{% for x, '2' in l %}{% endfor %}"

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["django"].from_string(template)

    assert (
        str(exc_info.value) == "'for' tag received an invalid argument: for x, '2' in l"
    )

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["rusty"].from_string(template)

    expected = """\
  × Invalid variable name '2' in for loop:
   ╭────
 1 │ {% for x, '2' in l %}{% endfor %}
   ·           ─┬─
   ·            ╰── invalid variable name
   ╰────
"""
    assert str(exc_info.value) == expected


def test_unexpected_expression_before_in():
    template = "{% for x y in l %}{% endfor %}"

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["django"].from_string(template)

    assert str(exc_info.value) == "'for' tag received an invalid argument: for x y in l"

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["rusty"].from_string(template)

    expected = """\
  × Unexpected expression in for loop. Did you miss a comma when unpacking?
   ╭────
 1 │ {% for x y in l %}{% endfor %}
   ·          ┬
   ·          ╰── unexpected expression
   ╰────
"""
    assert str(exc_info.value) == expected


def test_unexpected_expression_after_in():
    template = "{% for x in l m %}{% endfor %}"

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["django"].from_string(template)

    assert (
        str(exc_info.value)
        == "'for' statements should use the format 'for x in y': for x in l m"
    )

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["rusty"].from_string(template)

    expected = """\
  × Unexpected expression in for loop:
   ╭────
 1 │ {% for x in l m %}{% endfor %}
   ·               ┬
   ·               ╰── unexpected expression
   ╰────
"""
    assert str(exc_info.value) == expected


def test_unexpected_expression_after_reversed():
    template = "{% for x in l reversed m %}{% endfor %}"

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["django"].from_string(template)

    assert (
        str(exc_info.value)
        == "'for' statements should use the format 'for x in y': for x in l reversed m"
    )

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["rusty"].from_string(template)

    expected = """\
  × Unexpected expression in for loop:
   ╭────
 1 │ {% for x in l reversed m %}{% endfor %}
   ·                        ┬
   ·                        ╰── unexpected expression
   ╰────
"""
    assert str(exc_info.value) == expected
