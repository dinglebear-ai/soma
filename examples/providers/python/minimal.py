"""Minimal zero-dependency Soma Python provider."""


def greet(name: str) -> dict:
    """Greet one person."""
    return {"message": f"Hello, {name}!"}
