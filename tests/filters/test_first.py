import pytest
from django.template import engines
from django.template.exceptions import TemplateSyntaxError
from django.utils.safestring import mark_safe


def test_first_with_list():
    """Test first filter with a list."""
    template = "{{ items|first }}"
    context = {"items": ["a", "b", "c"]}

    django_template = engines["django"].from_string(template)
    rusty_template = engines["rusty"].from_string(template)

    assert django_template.render(context) == "a"
    assert rusty_template.render(context) == "a"


def test_first_with_list_falsy_value():
    """Test first filter returns falsy values correctly (Django's test_list)."""
    template = "{{ items|first }}"
    context = {"items": [0, 1, 2]}

    django_template = engines["django"].from_string(template)
    rusty_template = engines["rusty"].from_string(template)

    assert django_template.render(context) == "0"
    assert rusty_template.render(context) == "0"


def test_first_with_empty_list():
    """Test first filter with an empty list."""
    template = "{{ items|first }}"
    context = {"items": []}

    django_template = engines["django"].from_string(template)
    rusty_template = engines["rusty"].from_string(template)

    assert django_template.render(context) == ""
    assert rusty_template.render(context) == ""


def test_first_with_string():
    """Test first filter with a string."""
    template = "{{ text|first }}"
    context = {"text": "hello"}

    django_template = engines["django"].from_string(template)
    rusty_template = engines["rusty"].from_string(template)

    assert django_template.render(context) == "h"
    assert rusty_template.render(context) == "h"


def test_first_with_empty_string():
    """Test first filter with an empty string."""
    template = "{{ text|first }}"
    context = {"text": ""}

    django_template = engines["django"].from_string(template)
    rusty_template = engines["rusty"].from_string(template)

    assert django_template.render(context) == ""
    assert rusty_template.render(context) == ""


def test_first_with_none():
    """Test first filter with None value."""
    # Django's first filter raises TypeError when given None
    # We match this behavior for 1:1 compatibility
    template = "{{ items|first }}"
    context = {"items": None}

    # Both should raise TypeError
    with pytest.raises(TypeError) as django_exc:
        engines["django"].from_string(template).render(context)
    with pytest.raises(TypeError) as rusty_exc:
        engines["rusty"].from_string(template).render(context)

    # Check that error messages match
    assert "'NoneType' object is not subscriptable" in str(django_exc.value)
    assert "'NoneType' object is not subscriptable" in str(rusty_exc.value)


def test_first_with_tuple():
    """Test first filter with a tuple."""
    template = "{{ items|first }}"
    context = {"items": (1, 2, 3)}

    django_template = engines["django"].from_string(template)
    rusty_template = engines["rusty"].from_string(template)

    assert django_template.render(context) == "1"
    assert rusty_template.render(context) == "1"


def test_first_autoescape_on():
    """Test first filter with autoescape on (default)."""
    template = "{{ items|first }}"
    context = {"items": ["<b>bold</b>", "text"]}

    django_template = engines["django"].from_string(template)
    rusty_template = engines["rusty"].from_string(template)

    assert django_template.render(context) == "&lt;b&gt;bold&lt;/b&gt;"
    assert rusty_template.render(context) == "&lt;b&gt;bold&lt;/b&gt;"


def test_first_escaping_with_safe():
    """Test first filter with both escaped and safe strings (Django's first01)."""
    # Match Django's test exactly
    template = "{{ a|first }} {{ b|first }}"
    context = {"a": ["a&b", "x"], "b": [mark_safe("a&b"), "x"]}

    django_template = engines["django"].from_string(template)
    rusty_template = engines["rusty"].from_string(template)

    assert django_template.render(context) == "a&amp;b a&b"
    assert rusty_template.render(context) == "a&amp;b a&b"


def test_first_autoescape_off():
    """Test first filter with autoescape off."""
    template = "{% autoescape off %}{{ items|first }}{% endautoescape %}"
    context = {"items": ["<b>bold</b>", "text"]}

    django_template = engines["django"].from_string(template)
    rusty_template = engines["rusty"].from_string(template)

    assert django_template.render(context) == "<b>bold</b>"
    assert rusty_template.render(context) == "<b>bold</b>"


def test_first_autoescape_off_with_safe():
    """Test with autoescape off for both escaped and safe strings (Django's first02)."""
    # Match Django's test exactly
    template = "{% autoescape off %}{{ a|first }} {{ b|first }}{% endautoescape %}"
    context = {"a": ["a&b", "x"], "b": [mark_safe("a&b"), "x"]}

    django_template = engines["django"].from_string(template)
    rusty_template = engines["rusty"].from_string(template)

    assert django_template.render(context) == "a&b a&b"
    assert rusty_template.render(context) == "a&b a&b"


def test_first_with_safe_string():
    """Test first filter with mark_safe string."""
    template = "{{ items|first }}"
    context = {"items": [mark_safe("<b>bold</b>"), "text"]}

    django_template = engines["django"].from_string(template)
    rusty_template = engines["rusty"].from_string(template)

    # When the first item is marked safe, it should not be escaped
    assert django_template.render(context) == "<b>bold</b>"
    assert rusty_template.render(context) == "<b>bold</b>"


def test_first_no_argument():
    """Test that first filter doesn't accept arguments."""
    template = "{{ items|first:'arg' }}"
    context = {"items": [1, 2, 3]}

    with pytest.raises(TemplateSyntaxError) as django_exc:
        engines["django"].from_string(template).render(context)
    with pytest.raises(TemplateSyntaxError) as rusty_exc:
        engines["rusty"].from_string(template).render(context)

    # Check that both raise errors about arguments
    assert "first requires 1 arguments, 2 provided" in str(django_exc.value)
    assert "first filter does not take an argument" in str(rusty_exc.value)


def test_first_with_missing_variable():
    """Test first filter with missing variable."""
    template = "{{ missing|first }}"
    context = {}

    django_template = engines["django"].from_string(template)
    rusty_template = engines["rusty"].from_string(template)

    assert django_template.render(context) == ""
    assert rusty_template.render(context) == ""


def test_first_with_integer():
    """Test first filter with integer value."""
    # Django's first filter raises TypeError for integers
    template = "{{ num|first }}"
    context = {"num": 42}

    # Both should raise TypeError
    with pytest.raises(TypeError) as django_exc:
        engines["django"].from_string(template).render(context)
    with pytest.raises(TypeError) as rusty_exc:
        engines["rusty"].from_string(template).render(context)

    # Check that error messages match
    assert "'int' object is not subscriptable" in str(django_exc.value)
    assert "'int' object is not subscriptable" in str(rusty_exc.value)


def test_first_with_float():
    """Test first filter with float value."""
    # Django's first filter raises TypeError for floats
    template = "{{ num|first }}"
    context = {"num": 42.5}

    # Both should raise TypeError
    with pytest.raises(TypeError) as django_exc:
        engines["django"].from_string(template).render(context)
    with pytest.raises(TypeError) as rusty_exc:
        engines["rusty"].from_string(template).render(context)

    # Check that error messages match
    assert "'float' object is not subscriptable" in str(django_exc.value)
    assert "'float' object is not subscriptable" in str(rusty_exc.value)


def test_first_with_boolean():
    """Test first filter with boolean value."""
    template = "{{ value|first }}"

    # Test with True
    context = {"value": True}
    with pytest.raises(TypeError) as django_exc:
        engines["django"].from_string(template).render(context)
    with pytest.raises(TypeError) as rusty_exc:
        engines["rusty"].from_string(template).render(context)

    assert "'bool' object is not subscriptable" in str(django_exc.value)
    assert "'bool' object is not subscriptable" in str(rusty_exc.value)

    # Test with False
    context = {"value": False}
    with pytest.raises(TypeError) as django_exc:
        engines["django"].from_string(template).render(context)
    with pytest.raises(TypeError) as rusty_exc:
        engines["rusty"].from_string(template).render(context)

    assert "'bool' object is not subscriptable" in str(django_exc.value)
    assert "'bool' object is not subscriptable" in str(rusty_exc.value)


def test_first_with_custom_object_type_error():
    """Test first filter with custom object that raises TypeError on indexing."""

    class NotSubscriptable:
        def __getitem__(self, key):
            raise TypeError("custom type error")

    template = "{{ obj|first }}"
    context = {"obj": NotSubscriptable()}

    # Both should raise TypeError
    with pytest.raises(TypeError) as django_exc:
        engines["django"].from_string(template).render(context)
    with pytest.raises(TypeError) as rusty_exc:
        engines["rusty"].from_string(template).render(context)

    # Check that error messages match
    assert "custom type error" in str(django_exc.value)
    assert "custom type error" in str(rusty_exc.value)


def test_first_with_unicode_string():
    """Test first filter with unicode string."""
    template = "{{ text|first }}"
    context = {"text": "🎉🎊🎈"}

    django_template = engines["django"].from_string(template)
    rusty_template = engines["rusty"].from_string(template)

    assert django_template.render(context) == "🎉"
    assert rusty_template.render(context) == "🎉"
