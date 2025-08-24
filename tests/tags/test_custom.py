from django.template import engines


def test_simple_tag_double():
    template = "{% load double from custom_tags %}{% double 3 %}"

    django_template = engines["django"].from_string(template)
    rust_template = engines["rusty"].from_string(template)

    assert django_template.render({}) == "6"
    assert rust_template.render({}) == "6"
