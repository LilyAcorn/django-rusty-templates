def test_center(assert_render):
    template = "{{ var|center:5 }}"
    context = {"var": "123"}
    expected = " 123 "

    assert_render(template, context, expected)


def test_center_with_even_width(assert_render):
    template = "{{ var|center:15 }}"
    context = {"var": "Django"}
    expected = "     Django    "

    assert_render(template, context, expected)
