-- msgstore sintético no schema LEGADO (subconjunto das colunas reais).
-- Note que `media_wa_type` é TEXT, como nos bancos reais.

CREATE TABLE chat_list (_id INTEGER PRIMARY KEY AUTOINCREMENT, key_remote_jid TEXT UNIQUE,
    message_table_id INTEGER, subject TEXT, creation INTEGER, archived INTEGER);
CREATE TABLE messages (_id INTEGER PRIMARY KEY AUTOINCREMENT, key_remote_jid TEXT NOT NULL,
    key_from_me INTEGER, key_id TEXT NOT NULL, status INTEGER, needs_push INTEGER, data TEXT,
    timestamp INTEGER, media_url TEXT, media_mime_type TEXT, media_wa_type TEXT, media_size INTEGER,
    media_name TEXT, media_caption TEXT, media_hash TEXT, media_duration INTEGER, origin INTEGER,
    latitude REAL, longitude REAL, thumb_image TEXT, remote_resource TEXT, received_timestamp INTEGER,
    raw_data BLOB, starred INTEGER, quoted_row_id INTEGER, edit_version INTEGER, forwarded INTEGER);
CREATE TABLE messages_quotes (_id INTEGER PRIMARY KEY AUTOINCREMENT, key_remote_jid TEXT NOT NULL,
    key_from_me INTEGER, key_id TEXT NOT NULL, status INTEGER, data TEXT, timestamp INTEGER,
    media_wa_type TEXT, remote_resource TEXT);
CREATE TABLE messages_vcards (_id INTEGER PRIMARY KEY AUTOINCREMENT, message_row_id INTEGER,
    sender_jid TEXT, vcard TEXT, chat_jid TEXT);

INSERT INTO chat_list (key_remote_jid, subject, archived) VALUES
    ('5511944444444@s.whatsapp.net', NULL, 0),
    ('120363000000000002@g.us', 'Amigos', 0);

INSERT INTO messages_quotes (_id, key_remote_jid, key_from_me, key_id, data, media_wa_type) VALUES
    (1, '5511944444444@s.whatsapp.net', 0, 'L1', 'café da manhã', '0');

INSERT INTO messages (_id, key_remote_jid, key_from_me, key_id, status, data, timestamp, media_mime_type, media_wa_type, media_size, media_name, media_caption, latitude, longitude, remote_resource, raw_data, starred, quoted_row_id, edit_version) VALUES
    (1,  '-1', 0, '-1', -1, NULL, 0, NULL, '0', 0, NULL, NULL, 0, 0, NULL, NULL, 0, NULL, 0),
    (2,  '5511944444444@s.whatsapp.net', 0, 'L1', 0,  'café da manhã', 1000, NULL, '0', 0, NULL, NULL, 0, 0, NULL, NULL, 0, NULL, 0),
    (3,  '5511944444444@s.whatsapp.net', 1, 'L2', 13, 'bom dia', 2000, NULL, '0', 0, NULL, NULL, 0, 0, NULL, NULL, 1, 1, 0),
    (4,  '120363000000000002@g.us', 0, 'L3', 0, 'oi pessoal', 3000, NULL, '0', 0, NULL, NULL, 0, 0, '5511933333333@s.whatsapp.net', NULL, 0, NULL, 0),
    (5,  '5511944444444@s.whatsapp.net', 0, 'L4', 0, NULL, 4000, 'image/jpeg', '1', 54321, NULL, 'pôr do sol', 0, 0, NULL, X'FFD8FFE000104A464946', 0, NULL, 0),
    (6,  '5511944444444@s.whatsapp.net', 0, 'L5', 0, 'BEGIN:VCARD
FN:Zé
END:VCARD', 5000, NULL, '4', 0, 'Zé', NULL, 0, 0, NULL, NULL, 0, NULL, 0),
    (7,  '120363000000000002@g.us', 0, 'L6', 6, NULL, 6000, NULL, '0', 0, NULL, NULL, 0, 0, NULL, NULL, 0, NULL, 0),
    (8,  '5511944444444@s.whatsapp.net', 1, 'L7', 5, 'texto antigo', 7000, NULL, '0', 0, NULL, NULL, 0, 0, NULL, NULL, 0, NULL, 7),
    (9,  'status@broadcast', 0, 'L8', 0, 'meu status', 8000, NULL, '0', 0, NULL, NULL, 0, 0, '5511944444444@s.whatsapp.net', NULL, 0, NULL, 0),
    (10, '5511944444444@s.whatsapp.net', 0, 'L9', 0, NULL, 9000, NULL, '5', 0, NULL, NULL, -22.9519, -43.2105, NULL, NULL, 0, NULL, 0);
