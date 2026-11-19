-- Synthetic msgstore in the MODERN schema (subset of the real columns).
-- All numbers and names are fictitious.

CREATE TABLE jid (_id INTEGER PRIMARY KEY AUTOINCREMENT, user TEXT NOT NULL, server TEXT NOT NULL,
    agent INTEGER, device INTEGER, type INTEGER, raw_string TEXT);
CREATE TABLE jid_map (lid_row_id INTEGER PRIMARY KEY NOT NULL, jid_row_id INTEGER NOT NULL);
CREATE TABLE chat (_id INTEGER PRIMARY KEY AUTOINCREMENT, jid_row_id INTEGER UNIQUE, hidden INTEGER,
    subject TEXT, created_timestamp INTEGER, archived INTEGER, sort_timestamp INTEGER);
CREATE TABLE message (_id INTEGER PRIMARY KEY AUTOINCREMENT, chat_row_id INTEGER NOT NULL,
    from_me INTEGER NOT NULL, key_id TEXT NOT NULL, sender_jid_row_id INTEGER, status INTEGER,
    timestamp INTEGER, received_timestamp INTEGER, message_type INTEGER, text_data TEXT,
    starred INTEGER, sort_id INTEGER);
CREATE TABLE message_media (message_row_id INTEGER PRIMARY KEY, chat_row_id INTEGER, file_path TEXT,
    file_size INTEGER, mime_type TEXT, media_duration INTEGER, media_name TEXT, file_hash TEXT,
    width INTEGER, height INTEGER, raw_transcription_text TEXT);
CREATE TABLE message_thumbnail (message_row_id INTEGER PRIMARY KEY, thumbnail BLOB);
CREATE TABLE message_location (message_row_id INTEGER PRIMARY KEY, chat_row_id INTEGER, latitude REAL,
    longitude REAL, place_name TEXT, place_address TEXT, url TEXT);
CREATE TABLE message_quoted (message_row_id INTEGER PRIMARY KEY, chat_row_id INTEGER,
    parent_message_chat_row_id INTEGER, from_me INTEGER, sender_jid_row_id INTEGER, key_id TEXT,
    timestamp INTEGER, message_type INTEGER, text_data TEXT);
CREATE TABLE message_system (message_row_id INTEGER PRIMARY KEY, action_type INTEGER);
CREATE TABLE message_future (message_row_id INTEGER PRIMARY KEY, version INTEGER, data BLOB);
CREATE TABLE message_vcard (_id INTEGER PRIMARY KEY AUTOINCREMENT, message_row_id INTEGER, vcard TEXT);
CREATE TABLE message_add_on (_id INTEGER PRIMARY KEY AUTOINCREMENT, chat_row_id INTEGER, from_me INTEGER,
    key_id TEXT, sender_jid_row_id INTEGER, parent_message_row_id INTEGER, timestamp INTEGER,
    message_add_on_type INTEGER);
CREATE TABLE message_add_on_reaction (message_add_on_row_id INTEGER PRIMARY KEY, reaction TEXT,
    sender_timestamp INTEGER);

INSERT INTO jid (_id, user, server, type, raw_string) VALUES
    (2, '5511911111111', 's.whatsapp.net', 0, '5511911111111@s.whatsapp.net'),  -- Alice
    (3, '120363000000000001', 'g.us', 1, '120363000000000001@g.us'),              -- group
    (4, '98765', 'lid', 0, '98765@lid'),                                           -- Bob (LID)
    (5, '5511922222222', 's.whatsapp.net', 0, '5511922222222@s.whatsapp.net'),    -- Bob (phone)
    (6, 'status', 'broadcast', 11, 'status@broadcast'),
    (7, '55555', 'lid', 0, '55555@lid');                                           -- Carol (LID without mapping)
INSERT INTO jid_map VALUES (4, 5);

INSERT INTO chat (_id, jid_row_id, hidden, subject, archived) VALUES
    (1, 2, 0, NULL, 0),
    (2, 3, 0, 'Família', 1),
    (3, 6, 0, NULL, 0);

INSERT INTO message (_id, chat_row_id, from_me, key_id, sender_jid_row_id, status, timestamp, message_type, text_data, starred) VALUES
    (1,  1, 0, 'A1', 0, 13, 1000, 0,  'Oi, tudo bem? Vamos tomar um café', 0),
    (2,  1, 1, 'A2', 0, 13, 2000, 0,  'Tudo ótimo!', 1),
    (3,  1, 0, 'A3', 0, 13, 3000, 1,  'foto da praia', 0),
    (4,  1, 0, 'A4', 0, 13, 4000, 2,  NULL, 0),
    (5,  1, 1, 'A5', 0, 5,  5000, 15, 'segredo apagado', 0),
    (6,  1, 0, 'A6', 0, 0,  6000, 0,  'mensagem antiga apagada', 0),
    (7,  2, 0, 'G1', 4, 13, 7000, 0,  'olá grupo', 0),
    (8,  2, 0, 'G2', 7, 13, 8000, 0,  'sou a carol', 0),
    (9,  2, 0, 'G3', 4, 6,  9000, 7,  NULL, 0),
    (10, 1, 0, 'A7', 0, 13, 10000, 5, NULL, 0),
    (11, 1, 0, 'A8', 0, 13, 11000, 4, 'Zé', 0),
    (12, 3, 0, 'S1', 2, 13, 12000, 0, 'meu status', 0),
    (13, 99, 0, 'X1', 0, 13, 13000, 0, 'órfã', 0),
    (14, 1, 0, 'A9', 0, 13, 14000, 777, 'tipo novo', 0);

INSERT INTO message_media (message_row_id, chat_row_id, file_path, file_size, mime_type, media_duration, file_hash, width, height, raw_transcription_text) VALUES
    (3, 1, 'Media/WhatsApp Images/IMG-0001.jpg', 12345, 'image/jpeg', 0, 'aGFzaDE=', 1600, 1200, NULL),
    (4, 1, 'Media/WhatsApp Voice Notes/PTT-0001.opus', 2048, 'audio/ogg; codecs=opus', 4, 'aGFzaDI=', 0, 0, 'chego em dez minutos');
INSERT INTO message_thumbnail VALUES (3, X'FFD8FFE000104A464946');
INSERT INTO message_location (message_row_id, chat_row_id, latitude, longitude, place_name, place_address) VALUES
    (10, 1, -23.5614, -46.6559, 'Av. Paulista', 'São Paulo');
INSERT INTO message_quoted (message_row_id, chat_row_id, from_me, sender_jid_row_id, key_id, message_type, text_data) VALUES
    (2, 1, 0, 0, 'A1', 0, 'Oi, tudo bem? Vamos tomar um café');
INSERT INTO message_system VALUES (9, 4);
INSERT INTO message_future VALUES (6, 7, NULL);
INSERT INTO message_vcard (message_row_id, vcard) VALUES (11, 'BEGIN:VCARD
VERSION:3.0
FN:Zé
TEL:+55 11 90000-0000
END:VCARD');

INSERT INTO message_add_on (_id, chat_row_id, from_me, key_id, sender_jid_row_id, parent_message_row_id, timestamp, message_add_on_type) VALUES
    (1, 1, 1, 'R1', 0, 1, 1500, 56),
    (2, 2, 0, 'R2', 4, 7, 7500, 56),
    (3, 2, 0, 'R3', 7, 7, 7600, 56);
INSERT INTO message_add_on_reaction VALUES (1, '❤️', 1500), (2, '👍', 7500), (3, '', 7600);
