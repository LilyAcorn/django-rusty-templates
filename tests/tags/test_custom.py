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
