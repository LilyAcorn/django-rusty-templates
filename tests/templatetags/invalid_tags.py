from django import template


register = template.Library()


@register.simple_tag(takes_context=True)
def missing_context(request): ...


@register.simple_block_tag(takes_context=True)
def missing_context_block(content): ...


@register.simple_block_tag(
    takes_context=True, end_name="end_missing_content_block_with_context"
)
def missing_content_block_with_context(context): ...


@register.simple_block_tag(takes_context=False)
def missing_content_block(context): ...


@register.simple_tag(takes_context=True)
def request_path(context):
    global smuggled_context
    smuggled_context = context
    return context.request.path
