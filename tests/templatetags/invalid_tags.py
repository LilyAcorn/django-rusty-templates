from django import template


register = template.Library()


@register.simple_tag(takes_context=True)
def missing_context(request): ...


@register.simple_tag(takes_context=True)
def request_path(context):
    global smuggled_context
    smuggled_context = context
    return context.request.path
