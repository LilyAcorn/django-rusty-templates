from django.template import engines


def test_simple_block_tag_repeat():
    template = "{% load repeat from custom_tags %}{% repeat 5 %}foo{% endrepeat %}"

    django_template = engines["django"].from_string(template)
    rust_template = engines["rusty"].from_string(template)

    expected = "foofoofoofoofoo"
    assert django_template.render({}) == expected
    assert rust_template.render({}) == expected
