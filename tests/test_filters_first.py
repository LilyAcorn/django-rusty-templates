import pytest
from django.template import engines
from django.template.exceptions import TemplateSyntaxError


class TestFirstFilter:
    """Test the 'first' filter implementation."""

    def test_first_with_list(self):
        """Test first filter with a list."""
        rusty_engine = engines["rusty"]
        django_engine = engines["django"]

        template = "{{ items|first }}"
        context = {"items": ["a", "b", "c"]}

        rusty_result = rusty_engine.from_string(template).render(context)
        django_result = django_engine.from_string(template).render(context)

        assert rusty_result == "a"
        assert django_result == "a"

    def test_first_with_list_falsy_value(self):
        """Test first filter returns falsy values correctly (Django's test_list)."""
        rusty_engine = engines["rusty"]
        django_engine = engines["django"]

        template = "{{ items|first }}"
        context = {"items": [0, 1, 2]}

        rusty_result = rusty_engine.from_string(template).render(context)
        django_result = django_engine.from_string(template).render(context)

        assert rusty_result == "0"
        assert django_result == "0"

    def test_first_with_empty_list(self):
        """Test first filter with an empty list."""
        rusty_engine = engines["rusty"]
        django_engine = engines["django"]

        template = "{{ items|first }}"
        context = {"items": []}

        rusty_result = rusty_engine.from_string(template).render(context)
        django_result = django_engine.from_string(template).render(context)

        assert rusty_result == ""
        assert django_result == ""

    def test_first_with_string(self):
        """Test first filter with a string."""
        rusty_engine = engines["rusty"]
        django_engine = engines["django"]

        template = "{{ text|first }}"
        context = {"text": "hello"}

        rusty_result = rusty_engine.from_string(template).render(context)
        django_result = django_engine.from_string(template).render(context)

        assert rusty_result == "h"
        assert django_result == "h"

    def test_first_with_empty_string(self):
        """Test first filter with an empty string."""
        rusty_engine = engines["rusty"]
        django_engine = engines["django"]

        template = "{{ text|first }}"
        context = {"text": ""}

        rusty_result = rusty_engine.from_string(template).render(context)
        django_result = django_engine.from_string(template).render(context)

        assert rusty_result == ""
        assert django_result == ""

    def test_first_with_none(self):
        """Test first filter with None value."""
        # Django's first filter raises TypeError when given None
        # We match this behavior for 1:1 compatibility
        rusty_engine = engines["rusty"]
        django_engine = engines["django"]

        template = "{{ items|first }}"
        context = {"items": None}

        # Both should raise TypeError
        with pytest.raises(TypeError):
            rusty_engine.from_string(template).render(context)
        with pytest.raises(TypeError):
            django_engine.from_string(template).render(context)

    def test_first_with_tuple(self):
        """Test first filter with a tuple."""
        rusty_engine = engines["rusty"]
        django_engine = engines["django"]

        template = "{{ items|first }}"
        context = {"items": (1, 2, 3)}

        rusty_result = rusty_engine.from_string(template).render(context)
        django_result = django_engine.from_string(template).render(context)

        assert rusty_result == "1"
        assert django_result == "1"

    def test_first_autoescape_on(self):
        """Test first filter with autoescape on (default)."""
        rusty_engine = engines["rusty"]
        django_engine = engines["django"]

        template = "{{ items|first }}"
        context = {"items": ["<b>bold</b>", "text"]}

        rusty_result = rusty_engine.from_string(template).render(context)
        django_result = django_engine.from_string(template).render(context)

        assert rusty_result == "&lt;b&gt;bold&lt;/b&gt;"
        assert django_result == "&lt;b&gt;bold&lt;/b&gt;"

    def test_first_escaping_with_safe(self):
        """Test first filter with both escaped and safe strings (Django's first01)."""
        from django.utils.safestring import mark_safe

        rusty_engine = engines["rusty"]
        django_engine = engines["django"]

        # Match Django's test exactly
        template = "{{ a|first }} {{ b|first }}"
        context = {"a": ["a&b", "x"], "b": [mark_safe("a&b"), "x"]}

        rusty_result = rusty_engine.from_string(template).render(context)
        django_result = django_engine.from_string(template).render(context)

        assert rusty_result == "a&amp;b a&b"
        assert django_result == "a&amp;b a&b"

    def test_first_autoescape_off(self):
        """Test first filter with autoescape off."""
        rusty_engine = engines["rusty"]
        django_engine = engines["django"]

        template = "{% autoescape off %}{{ items|first }}{% endautoescape %}"
        context = {"items": ["<b>bold</b>", "text"]}

        rusty_result = rusty_engine.from_string(template).render(context)
        django_result = django_engine.from_string(template).render(context)

        assert rusty_result == "<b>bold</b>"
        assert django_result == "<b>bold</b>"

    def test_first_autoescape_off_with_safe(self):
        """Test with autoescape off for both escaped and safe strings (Django's first02)."""
        from django.utils.safestring import mark_safe

        rusty_engine = engines["rusty"]
        django_engine = engines["django"]

        # Match Django's test exactly
        template = "{% autoescape off %}{{ a|first }} {{ b|first }}{% endautoescape %}"
        context = {"a": ["a&b", "x"], "b": [mark_safe("a&b"), "x"]}

        rusty_result = rusty_engine.from_string(template).render(context)
        django_result = django_engine.from_string(template).render(context)

        assert rusty_result == "a&b a&b"
        assert django_result == "a&b a&b"

    def test_first_with_safe_string(self):
        """Test first filter with mark_safe string."""
        from django.utils.safestring import mark_safe

        rusty_engine = engines["rusty"]
        django_engine = engines["django"]

        template = "{{ items|first }}"
        context = {"items": [mark_safe("<b>bold</b>"), "text"]}

        # When the first item is marked safe, it should not be escaped
        rusty_result = rusty_engine.from_string(template).render(context)
        django_result = django_engine.from_string(template).render(context)

        assert rusty_result == "<b>bold</b>"
        assert django_result == "<b>bold</b>"

    def test_first_no_argument(self):
        """Test that first filter doesn't accept arguments."""
        rusty_engine = engines["rusty"]
        django_engine = engines["django"]

        template = "{{ items|first:'arg' }}"
        context = {"items": [1, 2, 3]}

        with pytest.raises(TemplateSyntaxError):
            rusty_engine.from_string(template).render(context)
        with pytest.raises(TemplateSyntaxError):
            django_engine.from_string(template).render(context)

    def test_first_with_missing_variable(self):
        """Test first filter with missing variable."""
        rusty_engine = engines["rusty"]
        django_engine = engines["django"]

        template = "{{ missing|first }}"
        context = {}

        rusty_result = rusty_engine.from_string(template).render(context)
        django_result = django_engine.from_string(template).render(context)

        assert rusty_result == ""
        assert django_result == ""

    def test_first_with_integer(self):
        """Test first filter with integer value."""
        # Django's first filter raises TypeError for integers
        rusty_engine = engines["rusty"]
        django_engine = engines["django"]

        template = "{{ num|first }}"
        context = {"num": 42}

        # Both should raise TypeError
        with pytest.raises(TypeError):
            rusty_engine.from_string(template).render(context)
        with pytest.raises(TypeError):
            django_engine.from_string(template).render(context)

    def test_first_with_float(self):
        """Test first filter with float value."""
        # Django's first filter raises TypeError for floats
        rusty_engine = engines["rusty"]
        django_engine = engines["django"]

        template = "{{ num|first }}"
        context = {"num": 42.5}

        # Both should raise TypeError
        with pytest.raises(TypeError):
            rusty_engine.from_string(template).render(context)
        with pytest.raises(TypeError):
            django_engine.from_string(template).render(context)
