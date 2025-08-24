from django import template


register = template.Library()


@register.simple_tag
def double(value):
    return value * 2


@register.simple_block_tag
def repeat(content, count):
    return content * count


@register.inclusion_tag("results.html")
def results(poll):
    return {"choices": poll.choices}
