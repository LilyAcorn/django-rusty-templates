from pathlib import Path

import pytest
from django.conf import settings
from django.template.engine import Engine
from django.template.library import InvalidTemplateLibrary
from django_rusty_templates import RustyTemplates


def test_import_libraries_import_error():
    params = {"libraries": {"import_error": "invalid.path"}}
    expected = "Invalid template library specified. ImportError raised when trying to load 'invalid.path': No module named 'invalid'"

    with pytest.raises(InvalidTemplateLibrary) as exc_info:
        Engine(**params)

    assert str(exc_info.value) == expected

    with pytest.raises(InvalidTemplateLibrary) as exc_info:
        RustyTemplates(
            {"OPTIONS": params, "NAME": "rust", "DIRS": [], "APP_DIRS": False}
        )

    assert str(exc_info.value) == expected


def test_import_libraries_no_register():
    params = {"libraries": {"no_register": "tests"}}
    expected = "Module  tests does not have a variable named 'register'"

    with pytest.raises(InvalidTemplateLibrary) as exc_info:
        Engine(**params)

    assert str(exc_info.value) == expected

    expected = "Module 'tests' does not have a variable named 'register'"
    with pytest.raises(InvalidTemplateLibrary) as exc_info:
        RustyTemplates(
            {"OPTIONS": params, "NAME": "rust", "DIRS": [], "APP_DIRS": False}
        )

    assert str(exc_info.value) == expected


def test_pathlib_dirs():
    engine = RustyTemplates(
        {
            "NAME": "rust",
            "OPTIONS": {},
            "DIRS": [Path(settings.BASE_DIR) / "templates"],
            "APP_DIRS": False,
        }
    )

    context = {"user": "Lily"}
    expected = "Hello Lily!\n"

    template = engine.get_template("basic.txt")
    assert template.render(context) == expected


def test_loader_priority():
    loaders = [
        "django.template.loaders.filesystem.Loader",
        "django.template.loaders.app_directories.Loader",
    ]

    engine = RustyTemplates(
        {
            "OPTIONS": {"loaders": loaders},
            "NAME": "rust",
            "DIRS": [],
            "APP_DIRS": False,
        }
    )

    context = {"user": "Lily"}
    expected = "Hello Lily!\n"

    template = engine.get_template("basic.txt")
    assert template.render(context) == expected


def test_cached_loader_priority():
    loaders = [
        (
            "django.template.loaders.cached.Loader",
            [
                "django.template.loaders.filesystem.Loader",
                "django.template.loaders.app_directories.Loader",
            ],
        ),
    ]

    engine = RustyTemplates(
        {
            "OPTIONS": {"loaders": loaders},
            "NAME": "rust",
            "DIRS": [],
            "APP_DIRS": False,
        }
    )

    context = {"user": "Lily"}
    expected = "Hello Lily!\n"

    template = engine.get_template("basic.txt")
    assert template.render(context) == expected


def test_locmem_loader():
    loaders = [
        (
            "django.template.loaders.locmem.Loader",
            {
                "index.html": "index",
            },
        )
    ]

    engine = RustyTemplates(
        {
            "OPTIONS": {"loaders": loaders},
            "NAME": "rust",
            "DIRS": [],
            "APP_DIRS": False,
        }
    )

    context = {"user": "Lily"}
    expected = "index"

    template = engine.get_template("index.html")
    assert template.render(context) == expected
