import os

BASE_DIR = os.path.dirname(os.path.abspath(__file__))

INSTALLED_APPS = [
    "tests.apps.DummyAppConfig",
]

TEMPLATES = [
    {
        "BACKEND": "django.template.backends.django.DjangoTemplates",
        "DIRS": ["tests/templates"],
        "OPTIONS": {
            "libraries": {
                "custom_filters": "tests.templatetags.custom_filters",
                "custom_tags": "tests.templatetags.custom_tags",
                "more_filters": "tests.templatetags.more_filters",
                "no_filters": "tests.templatetags.no_filters",
                "no_tags": "tests.templatetags.no_tags",
            },
        },
    },
    {
        "BACKEND": "django_rusty_templates.RustyTemplates",
        "DIRS": ["tests/templates"],
        "NAME": "rusty",
        "OPTIONS": {
            "libraries": {
                "custom_filters": "tests.templatetags.custom_filters",
                "custom_tags": "tests.templatetags.custom_tags",
                "more_filters": "tests.templatetags.more_filters",
                "no_filters": "tests.templatetags.no_filters",
                "no_tags": "tests.templatetags.no_tags",
            },
        },
    },
]

ROOT_URLCONF = "tests.urls"
