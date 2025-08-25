import pytest
from django.template import engines
from django.template.exceptions import TemplateSyntaxError


def test_simple_tag_double():
    template = "{% load double from custom_tags %}{% double 3 %}"

    django_template = engines["django"].from_string(template)
    rust_template = engines["rusty"].from_string(template)

    assert django_template.render({}) == "6"
    assert rust_template.render({}) == "6"


def test_simple_tag_double_kwarg():
    template = "{% load double from custom_tags %}{% double value=3 %}"

    django_template = engines["django"].from_string(template)
    rust_template = engines["rusty"].from_string(template)

    assert django_template.render({}) == "6"
    assert rust_template.render({}) == "6"


def test_simple_tag_kwargs():
    template = "{% load table from custom_tags %}{% table foo='bar' spam=1 %}"

    django_template = engines["django"].from_string(template)
    rust_template = engines["rusty"].from_string(template)

    assert django_template.render({}) == "foo-bar\nspam-1"
    assert rust_template.render({}) == "foo-bar\nspam-1"


def test_simple_tag_positional_and_kwargs():
    template = "{% load multiply from custom_tags %}{% multiply 3 b=2 c=4 %}"

    django_template = engines["django"].from_string(template)
    rust_template = engines["rusty"].from_string(template)

    assert django_template.render({}) == "24"
    assert rust_template.render({}) == "24"


def test_simple_tag_double_as_variable():
    template = "{% load double from custom_tags %}{% double 3 as foo %}{{ foo }}"

    django_template = engines["django"].from_string(template)
    rust_template = engines["rusty"].from_string(template)

    assert django_template.render({}) == "6"
    assert rust_template.render({}) == "6"


def test_simple_tag_double_kwarg_as_variable():
    template = "{% load double from custom_tags %}{% double value=3 as foo %}{{ foo }}"

    django_template = engines["django"].from_string(template)
    rust_template = engines["rusty"].from_string(template)

    assert django_template.render({}) == "6"
    assert rust_template.render({}) == "6"


def test_simple_tag_as_variable_after_default():
    template = "{% load invert from custom_tags %}{% invert as foo %}{{ foo }}"

    django_template = engines["django"].from_string(template)
    rust_template = engines["rusty"].from_string(template)

    assert django_template.render({}) == "0.5"
    assert rust_template.render({}) == "0.5"


def test_simple_tag_varargs():
    template = "{% load combine from custom_tags %}{% combine 2 3 4 as foo %}{{ foo }}"

    django_template = engines["django"].from_string(template)
    rust_template = engines["rusty"].from_string(template)

    assert django_template.render({}) == "9"
    assert rust_template.render({}) == "9"


def test_simple_tag_varargs_with_kwarg():
    template = "{% load combine from custom_tags %}{% combine 2 3 4 operation='multiply' as foo %}{{ foo }}"

    django_template = engines["django"].from_string(template)
    rust_template = engines["rusty"].from_string(template)

    assert django_template.render({}) == "24"
    assert rust_template.render({}) == "24"


def test_simple_tag_positional_after_kwarg():
    template = "{% load double from custom_tags %}{% double value=3 foo %}"

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["django"].from_string(template)

    assert str(exc_info.value) == "'double' received some positional argument(s) after some keyword argument(s)"

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["rusty"].from_string(template)

    assert str(exc_info.value) == """\
  × Unexpected positional argument after keyword argument
   ╭────
 1 │ {% load double from custom_tags %}{% double value=3 foo %}
   ·                                             ───┬─── ─┬─
   ·                                                │     ╰── this positional argument
   ·                                                ╰── after this keyword argument
   ╰────
"""


def test_simple_tag_too_many_positional_arguments():
    template = "{% load double from custom_tags %}{% double value foo %}"

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["django"].from_string(template)

    assert str(exc_info.value) == "'double' received too many positional arguments"

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["rusty"].from_string(template)

    assert str(exc_info.value) == """\
  × Unexpected positional argument
   ╭────
 1 │ {% load double from custom_tags %}{% double value foo %}
   ·                                                   ─┬─
   ·                                                    ╰── here
   ╰────
"""


def test_simple_tag_invalid_keyword_argument():
    template = "{% load double from custom_tags %}{% double foo=bar %}"

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["django"].from_string(template)

    assert str(exc_info.value) == "'double' received unexpected keyword argument 'foo'"

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["rusty"].from_string(template)

    assert str(exc_info.value) == """\
  × Unexpected keyword argument
   ╭────
 1 │ {% load double from custom_tags %}{% double foo=bar %}
   ·                                             ───┬───
   ·                                                ╰── here
   ╰────
"""


def test_simple_tag_missing_argument():
    template = "{% load double from custom_tags %}{% double %}"

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["django"].from_string(template)

    assert str(exc_info.value) == "'double' did not receive value(s) for the argument(s): 'value'"

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["rusty"].from_string(template)

    assert str(exc_info.value) == """\
  × 'double' did not receive value(s) for the argument(s): 'value'
   ╭────
 1 │ {% load double from custom_tags %}{% double %}
   ·                                            ▲
   ·                                            ╰── here
   ╰────
"""


def test_simple_tag_missing_arguments():
    template = "{% load multiply from custom_tags %}{% multiply %}"

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["django"].from_string(template)

    assert str(exc_info.value) == "'multiply' did not receive value(s) for the argument(s): 'a', 'b', 'c'"

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["rusty"].from_string(template)

    assert str(exc_info.value) == """\
  × 'multiply' did not receive value(s) for the argument(s): 'a', 'b', 'c'
   ╭────
 1 │ {% load multiply from custom_tags %}{% multiply %}
   ·                                                ▲
   ·                                                ╰── here
   ╰────
"""


def test_simple_tag_missing_arguments_with_kwarg():
    template = "{% load multiply from custom_tags %}{% multiply b=2 %}"

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["django"].from_string(template)

    assert str(exc_info.value) == "'multiply' did not receive value(s) for the argument(s): 'a', 'c'"

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["rusty"].from_string(template)

    assert str(exc_info.value) == """\
  × 'multiply' did not receive value(s) for the argument(s): 'a', 'c'
   ╭────
 1 │ {% load multiply from custom_tags %}{% multiply b=2 %}
   ·                                                 ─┬─
   ·                                                  ╰── here
   ╰────
"""


def test_simple_tag_duplicate_keyword_arguments():
    template = "{% load multiply from custom_tags %}{% multiply a=1 b=2 c=3 b=4 %}"

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["django"].from_string(template)

    assert str(exc_info.value) == "'multiply' received multiple values for keyword argument 'b'"

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["rusty"].from_string(template)

    assert str(exc_info.value) == """\
  × 'multiply' received multiple values for keyword argument 'b'
   ╭────
 1 │ {% load multiply from custom_tags %}{% multiply a=1 b=2 c=3 b=4 %}
   ·                                                     ─┬─     ─┬─
   ·                                                      │       ╰── second
   ·                                                      ╰── first
   ╰────
"""


def test_simple_tag_keyword_as_multiple_variables():
    template = "{% load double from custom_tags %}{% double value=1 as foo bar %}"

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["django"].from_string(template)

    assert str(exc_info.value) == "'double' received some positional argument(s) after some keyword argument(s)"

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["rusty"].from_string(template)

    assert str(exc_info.value) == """\
  × Unexpected positional argument after keyword argument
   ╭────
 1 │ {% load double from custom_tags %}{% double value=1 as foo bar %}
   ·                                             ───┬─── ─┬
   ·                                                │     ╰── this positional argument
   ·                                                ╰── after this keyword argument
   ╰────
"""


def test_simple_tag_positional_as_multiple_variables():
    template = "{% load double from custom_tags %}{% double value as foo bar %}"

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["django"].from_string(template)

    assert str(exc_info.value) == "'double' received too many positional arguments"

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["rusty"].from_string(template)

    assert str(exc_info.value) == """\
  × Unexpected positional argument
   ╭────
 1 │ {% load double from custom_tags %}{% double value as foo bar %}
   ·                                                   ─┬
   ·                                                    ╰── here
   ╰────
"""


def test_simple_tag_positional_as_multiple_variables_with_default():
    template = "{% load invert from custom_tags %}{% invert as foo bar %}"

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["django"].from_string(template)

    assert str(exc_info.value) == "'invert' received too many positional arguments"

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["rusty"].from_string(template)

    assert str(exc_info.value) == """\
  × Unexpected positional argument
   ╭────
 1 │ {% load invert from custom_tags %}{% invert as foo bar %}
   ·                                                ─┬─
   ·                                                 ╰── here
   ╰────
"""


def test_simple_tag_keyword_missing_target_variable():
    template = "{% load double from custom_tags %}{% double value=1 as %}"

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["django"].from_string(template)

    assert str(exc_info.value) == "'double' received some positional argument(s) after some keyword argument(s)"

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["rusty"].from_string(template)

    assert str(exc_info.value) == """\
  × Unexpected positional argument after keyword argument
   ╭────
 1 │ {% load double from custom_tags %}{% double value=1 as %}
   ·                                             ───┬─── ─┬
   ·                                                │     ╰── this positional argument
   ·                                                ╰── after this keyword argument
   ╰────
"""


def test_simple_tag_positional_missing_target_variable():
    template = "{% load double from custom_tags %}{% double value as %}"

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["django"].from_string(template)

    assert str(exc_info.value) == "'double' received too many positional arguments"

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["rusty"].from_string(template)

    assert str(exc_info.value) == """\
  × Unexpected positional argument
   ╭────
 1 │ {% load double from custom_tags %}{% double value as %}
   ·                                                   ─┬
   ·                                                    ╰── here
   ╰────
"""


def test_simple_tag_incomplete_keyword_argument():
    template = "{% load double from custom_tags %}{% double value= %}"

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["django"].from_string(template)

    assert str(exc_info.value) == "Could not parse the remainder: '=' from 'value='"

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["rusty"].from_string(template)

    assert str(exc_info.value) == """\
  × Incomplete keyword argument
   ╭────
 1 │ {% load double from custom_tags %}{% double value= %}
   ·                                             ───┬──
   ·                                                ╰── here
   ╰────
"""


def test_simple_tag_invalid_filter():
    template = "{% load double from custom_tags %}{% double foo|bar %}"

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["django"].from_string(template)

    assert str(exc_info.value) == "Invalid filter: 'bar'"

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["rusty"].from_string(template)

    assert str(exc_info.value) == """\
  × Invalid filter: 'bar'
   ╭────
 1 │ {% load double from custom_tags %}{% double foo|bar %}
   ·                                                 ─┬─
   ·                                                  ╰── here
   ╰────
"""


def test_simple_tag_invalid_filter_in_keyword_argument():
    template = "{% load double from custom_tags %}{% double value=foo|bar %}"

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["django"].from_string(template)

    assert str(exc_info.value) == "Invalid filter: 'bar'"

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["rusty"].from_string(template)

    assert str(exc_info.value) == """\
  × Invalid filter: 'bar'
   ╭────
 1 │ {% load double from custom_tags %}{% double value=foo|bar %}
   ·                                                       ─┬─
   ·                                                        ╰── here
   ╰────
"""
