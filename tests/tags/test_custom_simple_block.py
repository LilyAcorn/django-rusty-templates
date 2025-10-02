import pytest
from django.template import engines
from django.template.base import VariableDoesNotExist
from django.template.exceptions import TemplateSyntaxError


def test_simple_block_tag_repeat():
    template = "{% load repeat from custom_tags %}{% repeat 5 %}foo{% endrepeat %}"

    django_template = engines["django"].from_string(template)
    rust_template = engines["rusty"].from_string(template)

    expected = "foofoofoofoofoo"
    assert django_template.render({}) == expected
    assert rust_template.render({}) == expected


def test_with_block():
    template = "{% load with_block from custom_tags %}{% with_block var='name' %}{{ user }}{% end_with_block %}{{ name|lower }}"

    django_template = engines["django"].from_string(template)
    rust_template = engines["rusty"].from_string(template)

    context = {"user": "Lily"}
    expected = "lily"
    assert django_template.render(context) == expected
    assert rust_template.render(context) == expected


def test_simple_block_tag_missing_context():
    template = "{% load missing_context_block from invalid_tags %}{% missing_context_block %}{% end_missing_context_block %}"

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["django"].from_string(template)

    assert (
        str(exc_info.value)
        == "'missing_context_block' is decorated with takes_context=True so it must have a first argument of 'context' and a second argument of 'content'"
    )

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["rusty"].from_string(template)

    assert (
        str(exc_info.value)
        == """\
  × 'missing_context_block' is decorated with takes_context=True so it must
  │ have a first argument of 'context' and a second argument of 'content'
   ╭────
 1 │ {% load missing_context_block from invalid_tags %}{% missing_context_block %}{% end_missing_context_block %}
   ·         ──────────┬──────────
   ·                   ╰── loaded here
   ╰────
"""
    )


def test_simple_block_tag_missing_content():
    template = "{% load missing_content_block from invalid_tags %}{% missing_content_block %}{% end_missing_content_block %}"

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["django"].from_string(template)

    assert (
        str(exc_info.value)
        == "'missing_content_block' must have a first argument of 'content'"
    )

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["rusty"].from_string(template)

    assert (
        str(exc_info.value)
        == """\
  × 'missing_content_block' must have a first argument of 'content'
   ╭────
 1 │ {% load missing_content_block from invalid_tags %}{% missing_content_block %}{% end_missing_content_block %}
   ·         ──────────┬──────────
   ·                   ╰── loaded here
   ╰────
"""
    )


def test_simple_block_tag_missing_content_takes_context():
    template = "{% load missing_content_block_with_context from invalid_tags %}{% missing_content_block_with_context %}{% end_missing_content_block_with_context %}"

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["django"].from_string(template)

    assert (
        str(exc_info.value)
        == "'missing_content_block_with_context' is decorated with takes_context=True so it must have a first argument of 'context' and a second argument of 'content'"
    )

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["rusty"].from_string(template)

    assert (
        str(exc_info.value)
        == """\
  × 'missing_content_block_with_context' is decorated with takes_context=True
  │ so it must have a first argument of 'context' and a second argument of
  │ 'content'
   ╭────
 1 │ {% load missing_content_block_with_context from invalid_tags %}{% missing_content_block_with_context %}{% end_missing_content_block_with_context %}
   ·         ─────────────────┬────────────────
   ·                          ╰── loaded here
   ╰────
"""
    )


def test_simple_block_tag_missing_end_tag():
    template = "{% load repeat from custom_tags %}{% repeat 3 %}"

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["django"].from_string(template)

    assert (
        str(exc_info.value)
        == "Unclosed tag on line 1: 'repeat'. Looking for one of: endrepeat."
    )

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["rusty"].from_string(template)

    assert (
        str(exc_info.value)
        == """\
  × Unclosed 'repeat' tag. Looking for one of: endrepeat
   ╭────
 1 │ {% load repeat from custom_tags %}{% repeat 3 %}
   ·                                   ───────┬──────
   ·                                          ╰── started here
   ╰────
"""
    )


def test_simple_block_tag_end_tag_only():
    template = "{% load repeat from custom_tags %}{% endrepeat %}"

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["django"].from_string(template)

    assert (
        str(exc_info.value)
        == "Invalid block tag on line 1: 'endrepeat'. Did you forget to register or load this tag?"
    )

    with pytest.raises(TemplateSyntaxError) as exc_info:
        engines["rusty"].from_string(template)

    assert (
        str(exc_info.value)
        == """\
  × Unexpected tag endrepeat
   ╭────
 1 │ {% load repeat from custom_tags %}{% endrepeat %}
   ·                                   ───────┬───────
   ·                                          ╰── unexpected tag
   ╰────
"""
    )


def test_simple_block_tag_missing_argument():
    template = "{% load repeat from custom_tags %}{% repeat five %}{% endrepeat %}"

    django_template = engines["django"].from_string(template)
    rust_template = engines["rusty"].from_string(template)

    with pytest.raises(TypeError) as exc_info:
        django_template.render({})

    assert str(exc_info.value) == "can't multiply sequence by non-int of type 'str'"

    with pytest.raises(TypeError) as exc_info:
        rust_template.render({})

    assert (
        str(exc_info.value)
        == """\
  × can't multiply sequence by non-int of type 'str'
   ╭────
 1 │ {% load repeat from custom_tags %}{% repeat five %}{% endrepeat %}
   ·                                   ────────┬────────
   ·                                           ╰── here
   ╰────
"""
    )


def test_simple_block_tag_invalid_argument():
    template = "{% load repeat from custom_tags %}{% repeat five|default:five %}{% endrepeat %}"

    django_template = engines["django"].from_string(template)
    rust_template = engines["rusty"].from_string(template)

    with pytest.raises(VariableDoesNotExist) as exc_info:
        django_template.render({})

    assert (
        str(exc_info.value)
        == "Failed lookup for key [five] in [{'True': True, 'False': False, 'None': None}, {}]"
    )

    with pytest.raises(VariableDoesNotExist) as exc_info:
        rust_template.render({})

    assert (
        str(exc_info.value)
        == """\
  × Failed lookup for key [five] in {"False": False, "None": None, "True":
  │ True}
   ╭────
 1 │ {% load repeat from custom_tags %}{% repeat five|default:five %}{% endrepeat %}
   ·                                                          ──┬─
   ·                                                            ╰── key
   ╰────
"""
    )
