import pytest
from django.template import engines
from django.template.base import VariableDoesNotExist
from django.template.exceptions import TemplateSyntaxError
from django.test import RequestFactory
from django.urls import resolve, NoReverseMatch


factory = RequestFactory()


def test_render_url():
    template = "{% url 'home' %}"
    django_template = engines["django"].from_string(template)
    rust_template = engines["rusty"].from_string(template)

    expected = "/"
    assert django_template.render({}) == expected
    assert rust_template.render({}) == expected


def test_render_url_variable():
    template = "{% url home %}"
    django_template = engines["django"].from_string(template)
    rust_template = engines["rusty"].from_string(template)

    expected = "/"
    assert django_template.render({"home": "home"}) == expected
    assert rust_template.render({"home": "home"}) == expected


def test_render_url_variable_missing():
    template = "{% url home %}"
    django_template = engines["django"].from_string(template)
    rust_template = engines["rusty"].from_string(template)

    with pytest.raises(NoReverseMatch) as django_error:
        django_template.render({})

    with pytest.raises(NoReverseMatch) as rust_error:
        rust_template.render({})

    msg = "Reverse for '' not found. '' is not a valid view function or pattern name."
    assert django_error.value.args[0] == msg
    assert rust_error.value.args[0] == msg


def test_render_url_view_missing_as():
    template = "{% url 'missing' as missing %}{{ missing }}"
    django_template = engines["django"].from_string(template)
    rust_template = engines["rusty"].from_string(template)

    expected = ""
    assert django_template.render({}) == expected
    assert rust_template.render({}) == expected


def test_render_url_arg():
    template = "{% url 'bio' 'lily' %}"
    django_template = engines["django"].from_string(template)
    rust_template = engines["rusty"].from_string(template)

    expected = "/bio/lily/"
    assert django_template.render({}) == expected
    assert rust_template.render({}) == expected


def test_render_url_kwarg():
    template = "{% url 'bio' username='lily' %}"
    django_template = engines["django"].from_string(template)
    rust_template = engines["rusty"].from_string(template)

    expected = "/bio/lily/"
    assert django_template.render({}) == expected
    assert rust_template.render({}) == expected


def test_render_url_arg_as_variable():
    template = "{% url 'bio' 'lily' as bio %}https://example.com{{ bio }}"
    django_template = engines["django"].from_string(template)
    rust_template = engines["rusty"].from_string(template)

    expected = "https://example.com/bio/lily/"
    assert django_template.render({}) == expected
    assert rust_template.render({}) == expected


def test_render_url_kwarg_as_variable():
    template = "{% url 'bio' username='lily' as bio %}https://example.com{{ bio }}"
    django_template = engines["django"].from_string(template)
    rust_template = engines["rusty"].from_string(template)

    expected = "https://example.com/bio/lily/"
    assert django_template.render({}) == expected
    assert rust_template.render({}) == expected


def test_render_url_current_app_unset():
    template = "{% url 'users:user' 'lily' %}"
    django_template = engines["django"].from_string(template)
    rust_template = engines["rusty"].from_string(template)

    request = factory.get("/")

    expected = "/users/lily/"
    assert django_template.render({}, request) == expected
    assert rust_template.render({}, request) == expected


def test_render_url_current_app():
    template = "{% url 'users:user' 'lily' %}"
    django_template = engines["django"].from_string(template)
    rust_template = engines["rusty"].from_string(template)

    request = factory.get("/")
    request.current_app = "members"

    expected = "/members/lily/"
    assert django_template.render({}, request) == expected
    assert rust_template.render({}, request) == expected


def test_render_url_current_app_kwargs():
    template = "{% url 'users:user' username='lily' %}"
    django_template = engines["django"].from_string(template)
    rust_template = engines["rusty"].from_string(template)

    request = factory.get("/")
    request.current_app = "members"

    expected = "/members/lily/"
    assert django_template.render({}, request) == expected
    assert rust_template.render({}, request) == expected


def test_render_url_current_app_resolver_match():
    template = "{% url 'users:user' username='lily' %}"
    django_template = engines["django"].from_string(template)
    rust_template = engines["rusty"].from_string(template)

    request = factory.get("/")
    request.resolver_match = resolve("/members/bryony/")

    expected = "/members/lily/"
    assert django_template.render({}, request) == expected
    assert rust_template.render({}, request) == expected


def test_render_url_view_name_error():
    template = "{% url foo.bar.1b.baz %}"

    django_template = engines["django"].from_string(template)
    rust_template = engines["rusty"].from_string(template)

    with pytest.raises(NoReverseMatch) as django_error:
        django_template.render({"foo": {"bar": 1}})

    msg = "Reverse for '' not found. '' is not a valid view function or pattern name."
    assert django_error.value.args[0] == msg

    with pytest.raises(VariableDoesNotExist) as rust_error:
        rust_template.render({"foo": {"bar": 1}})

    expected = """\
  × Failed lookup for key [1b] in 1
   ╭────
 1 │ {% url foo.bar.1b.baz %}
   ·        ───┬─── ─┬
   ·           │     ╰── key
   ·           ╰── 1
   ╰────
"""
    assert str(rust_error.value) == expected


def test_render_url_invalid_keyword():
    template = "{% url foo= %}"

    with pytest.raises(TemplateSyntaxError) as django_error:
        engines["django"].from_string(template)

    msg = "Could not parse the remainder: '=' from 'foo='"
    assert str(django_error.value) == msg

    with pytest.raises(TemplateSyntaxError) as rust_error:
        engines["rusty"].from_string(template)

    expected = """\
  × Incomplete keyword argument
   ╭────
 1 │ {% url foo= %}
   ·        ──┬─
   ·          ╰── here
   ╰────
"""
    assert str(rust_error.value) == expected


def test_render_url_invalid_dotted_lookup_keyword():
    template = "{% url foo.bar= %}"

    with pytest.raises(TemplateSyntaxError) as django_error:
        engines["django"].from_string(template)

    msg = "Could not parse the remainder: '=' from 'foo.bar='"
    assert str(django_error.value) == msg

    with pytest.raises(TemplateSyntaxError) as rust_error:
        engines["rusty"].from_string(template)

    expected = """\
  × Could not parse the remainder
   ╭────
 1 │ {% url foo.bar= %}
   ·               ┬
   ·               ╰── here
   ╰────
"""
    assert str(rust_error.value) == expected


def test_render_url_dotted_lookup_keyword():
    template = "{% url foo.bar='lily' %}"

    with pytest.raises(TemplateSyntaxError) as django_error:
        engines["django"].from_string(template)

    msg = "Could not parse the remainder: '='lily'' from 'foo.bar='lily''"
    assert str(django_error.value) == msg

    with pytest.raises(TemplateSyntaxError) as rust_error:
        engines["rusty"].from_string(template)

    expected = """\
  × Could not parse the remainder
   ╭────
 1 │ {% url foo.bar='lily' %}
   ·               ───┬───
   ·                  ╰── here
   ╰────
"""
    assert str(rust_error.value) == expected


def test_render_url_dotted_lookup_filter_with_equal_char():
    template = "{% url foo.bar|default:'=' %}"

    django_template = engines["django"].from_string(template)
    rust_template = engines["rusty"].from_string(template)

    with pytest.raises(NoReverseMatch) as django_error:
        django_template.render({})

    msg = "Reverse for '=' not found. '=' is not a valid view function or pattern name."
    assert django_error.value.args[0] == msg

    with pytest.raises(NoReverseMatch) as rust_error:
        rust_template.render({})

    assert rust_error.value.args[0] == msg
