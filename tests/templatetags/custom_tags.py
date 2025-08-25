from django import template


register = template.Library()


@register.simple_tag
def double(value):
    return value * 2


@register.simple_tag
def multiply(a, b, c):
    return a * b * c


@register.simple_tag
def invert(value=2):
    return 1 / value


@register.simple_tag
def combine(*args, operation="add"):
    if operation == "add":
        return sum(args)

    if operation == "multiply":
        total = 1
        for arg in args:
            total *= arg
        return total

    raise RuntimeError("Unknown operation")


@register.simple_tag
def table(**kwargs):
    return "\n".join(f"{k}-{v}" for k, v in kwargs.items())
#
#
#@register.simple_block_tag
#def repeat(content, count):
#    return content * count
#
#
#@register.inclusion_tag("results.html")
#def results(poll):
#    return {"choices": poll.choices}
