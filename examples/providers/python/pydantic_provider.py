# /// script
# requires-python = ">=3.11"
# dependencies = ["pydantic>=2.11,<3"]
# ///
"""Provider using a Pydantic model for explicit schema metadata."""

from pydantic import BaseModel, Field
from soma_provider import provider, tool

PROVIDER = provider(name="pydantic-example", kind="python")


class ForecastRequest(BaseModel):
    city: str = Field(description="City to forecast")
    days: int = Field(default=1, ge=1, le=7)


@tool(
    description="Return a demonstration forecast.",
    input_schema=ForecastRequest.model_json_schema(),
)
def forecast(city: str, days: int = 1) -> dict:
    return {"city": city, "days": days, "conditions": "demo"}
