import pytest
from django.template import engines
from django.template.base import VariableDoesNotExist
from django.template.exceptions import TemplateSyntaxError


def test_center(assert_render):
    template = "{{ var|center:5 }}"
    context = {"var": "123"}
    expected = " 123 "

    assert_render(template, context, expected)


def test_center_with_odd_width_as_django_test_it(assert_render):
    template = "{{ var|center:15 }}"
    context = {"var": "Django"}
    expected = "     Django    "

    assert_render(template, context, expected)


def test_center_with_even_width(assert_render):
    template = "{{ var|center:6 }}"
    context = {"var": "odd"}
    expected = " odd  "

    assert_render(template, context, expected)


def test_center_with_odd_width(assert_render):
    template = "{{ var|center:7 }}"
    context = {"var": "even"}
    expected = "  even "

    assert_render(template, context, expected)


def test_add_no_argument():
    template = "{{ foo|center }}"
    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["django"].from_string(template)

    assert str(exc_info.value) == "center requires 2 arguments, 1 provided"

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["rusty"].from_string(template)

    expected = """\
  × Expected an argument
   ╭────
 1 │ {{ foo|center }}
   ·        ───┬──
   ·           ╰── here
   ╰────
"""
    assert str(exc_info.value) == expected


def test_argument_not_integer():
    template = "{{ foo|center:bar }}"
    expected = "invalid literal for int() with base 10: 'not an integer'"
    with pytest.raises(ValueError) as exc_info:
        engines["django"].from_string(template).render({"foo": "test", "bar": "not an integer"})

    assert str(exc_info.value) == expected

    with pytest.raises(ValueError) as exc_info:
        engines["rusty"].from_string(template).render({"foo": "test", "bar": "not an integer"})

    assert str(exc_info.value) == expected


def test_center_argument_less_than_string_length(assert_render):
    template = "{{ foo|center:2 }}"
    context = {"foo": "test"}
    expected = "test"  # No padding since the width is less than the string length

    assert_render(template, context, expected)


def test_center_argument_float(assert_render):
    template = "{{ foo|center:6.5 }}"
    context = {"foo": "test"}
    expected = " test "

    assert_render(template, context, expected)


def test_center_argument_negative(assert_render):
    template = "{{ foo|center:-5 }}"
    context = {"foo": "test"}
    expected = "test"  # No padding since the width is negative

    assert_render(template, context, expected)
