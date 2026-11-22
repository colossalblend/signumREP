from __future__ import annotations

from collections.abc import Sequence

from aiogram.types import MessageEntity


def replace_custom_emojis(text: str, entities: Sequence[MessageEntity]) -> str:
    source = text.encode("utf-16-le")
    parts = []
    cursor = 0
    for entity in sorted(entities, key=lambda item: item.offset):
        if entity.type != "custom_emoji" or not entity.custom_emoji_id:
            continue
        start = entity.offset * 2
        end = (entity.offset + entity.length) * 2
        if start < cursor or end > len(source) or start >= end:
            continue
        parts.extend((source[cursor:start].decode("utf-16-le"), entity.custom_emoji_id))
        cursor = end
    parts.append(source[cursor:].decode("utf-16-le"))
    return "".join(parts)


def has_custom_emoji(message) -> bool:
    entities = (
        message.entities if message.text is not None else message.caption_entities
    )
    return any(
        entity.type == "custom_emoji" and entity.custom_emoji_id
        for entity in entities or []
    )
