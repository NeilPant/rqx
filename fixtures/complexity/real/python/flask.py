# Functions copied unchanged from https://github.com/pallets/flask at commit 49943b5a94413a530761350f04e896f76e8d620e.
# BSD-3-Clause. Copyright 2010 Pallets.
# The license text is in ../NOTICE. Names a function uses that the project defines elsewhere are left undefined. A method is dedented out of its class.


# From pallets/flask src/flask/cli.py:41 at 49943b5a9441.
# @complexity time=O(n) space=O(n) conf=heuristic
def find_best_app(module: ModuleType) -> Flask:
    """Given a module instance this tries to find the best possible
    application in the module or raises an exception.
    """
    from . import Flask

    # Search for the most common names first.
    for attr_name in ("app", "application"):
        app = getattr(module, attr_name, None)

        if isinstance(app, Flask):
            return app

    # Otherwise find the only object that is a Flask instance.
    matches = [v for v in module.__dict__.values() if isinstance(v, Flask)]

    if len(matches) == 1:
        return matches[0]
    elif len(matches) > 1:
        raise NoAppException(
            "Detected multiple Flask applications in module"
            f" '{module.__name__}'. Use '{module.__name__}:name'"
            " to specify the correct one."
        )

    # Search for app factory functions.
    for attr_name in ("create_app", "make_app"):
        app_factory = getattr(module, attr_name, None)

        if inspect.isfunction(app_factory):
            try:
                app = app_factory()

                if isinstance(app, Flask):
                    return app
            except TypeError as e:
                if not _called_with_wrong_args(app_factory):
                    raise

                raise NoAppException(
                    f"Detected factory '{attr_name}' in module '{module.__name__}',"
                    " but could not call it without arguments. Use"
                    f" '{module.__name__}:{attr_name}(args)'"
                    " to specify arguments."
                ) from e

    raise NoAppException(
        "Failed to find Flask application or factory in module"
        f" '{module.__name__}'. Use '{module.__name__}:name'"
        " to specify one."
    )


# From pallets/flask src/flask/cli.py:94 at 49943b5a9441.
# @complexity time=O(n) space=O(1) conf=heuristic
def _called_with_wrong_args(f: t.Callable[..., Flask]) -> bool:
    """Check whether calling a function raised a ``TypeError`` because
    the call failed or because something in the factory raised the
    error.

    :param f: The function that was called.
    :return: ``True`` if the call failed.
    """
    tb = sys.exc_info()[2]

    try:
        while tb is not None:
            if tb.tb_frame.f_code is f.__code__:
                # In the function, it was called successfully.
                return False

            tb = tb.tb_next

        # Didn't reach the function.
        return True
    finally:
        # Delete tb to break a circular reference.
        # https://docs.python.org/2/library/sys.html#sys.exc_info
        del tb


# From pallets/flask src/flask/cli.py:200 at 49943b5a9441.
# @complexity time=O(n^2) space=O(n) conf=heuristic
def prepare_import(path: str) -> str:
    """Given a filename this will try to calculate the python path, add it
    to the search path and return the actual module name that is expected.
    """
    path = os.path.realpath(path)

    fname, ext = os.path.splitext(path)
    if ext == ".py":
        path = fname

    if os.path.basename(path) == "__init__":
        path = os.path.dirname(path)

    module_name = []

    # move up until outside package structure (no __init__.py)
    while True:
        path, name = os.path.split(path)
        module_name.append(name)

        if not os.path.exists(os.path.join(path, "__init__.py")):
            break

    if sys.path[0] != path:
        sys.path.insert(0, path)

    return ".".join(module_name[::-1])


# From pallets/flask src/flask/sansio/app.py:869 at 49943b5a9441.
# @complexity time=O(n*m) space=O(n) conf=heuristic
def _find_error_handler(
    self, e: Exception, blueprints: list[str]
) -> ft.ErrorHandlerCallable | None:
    """Return a registered error handler for an exception in this order:
    blueprint handler for a specific code, app handler for a specific code,
    blueprint handler for an exception class, app handler for an exception
    class, or ``None`` if a suitable handler is not found.
    """
    exc_class, code = self._get_exc_class_and_code(type(e))
    names = (*blueprints, None)

    for c in (code, None) if code is not None else (None,):
        for name in names:
            handler_map = self.error_handler_spec[name][c]

            if not handler_map:
                continue

            for cls in exc_class.__mro__:
                handler = handler_map.get(cls)

                if handler is not None:
                    return handler
    return None


# From pallets/flask src/flask/helpers.py:645 at 49943b5a9441.
# @complexity time=O(n^2) space=O(n^2) conf=heuristic
def _split_blueprint_path(name: str) -> list[str]:
    out: list[str] = [name]

    if "." in name:
        out.extend(_split_blueprint_path(name.rpartition(".")[0]))

    return out
