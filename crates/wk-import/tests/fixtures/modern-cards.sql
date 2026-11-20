-- Synthetic additions on top of modern.sql: the kinds mapped from a real
-- backup's structure (see wk-import/src/probe.rs). All content is fictitious.

CREATE TABLE message_poll (message_row_id INTEGER PRIMARY KEY, selectable_options_count INTEGER);
CREATE TABLE message_poll_option (_id INTEGER PRIMARY KEY AUTOINCREMENT, message_row_id INTEGER,
    option_name TEXT, vote_total INTEGER);
CREATE TABLE message_event (message_row_id INTEGER PRIMARY KEY, chat_row_id INTEGER, name TEXT,
    description TEXT, start_time INTEGER, end_time INTEGER, location_name TEXT, location_address TEXT,
    is_canceled INTEGER, is_schedule_call INTEGER);
CREATE TABLE message_product (message_row_id INTEGER PRIMARY KEY, title TEXT, description TEXT,
    amount_1000 INTEGER, currency_code TEXT, body TEXT);
CREATE TABLE message_group_invite (message_row_id INTEGER PRIMARY KEY, group_name TEXT);
CREATE TABLE message_template (message_row_id INTEGER PRIMARY KEY, content_text_data TEXT,
    footer_text_data TEXT);
CREATE TABLE message_template_button (_id INTEGER PRIMARY KEY AUTOINCREMENT, message_row_id INTEGER,
    text_data TEXT);
CREATE TABLE message_ui_elements (_id INTEGER PRIMARY KEY AUTOINCREMENT, message_row_id INTEGER,
    element_type INTEGER, element_content TEXT, button_text TEXT, description TEXT, footer_text TEXT);
CREATE TABLE message_ephemeral_setting (message_row_id INTEGER PRIMARY KEY, setting_duration INTEGER);
CREATE TABLE message_limit_sharing_setting (message_row_id INTEGER, enabled INTEGER, trigger INTEGER);
CREATE TABLE group_history_metadata (message_row_id INTEGER PRIMARY KEY, message_count INTEGER);
CREATE TABLE message_album (message_row_id INTEGER PRIMARY KEY, image_count INTEGER);
CREATE TABLE message_association (_id INTEGER PRIMARY KEY AUTOINCREMENT, association_type INTEGER,
    parent_message_row_id INTEGER, child_message_row_id INTEGER);
CREATE TABLE message_revoked (message_row_id INTEGER PRIMARY KEY, revoked_key_id TEXT);

INSERT INTO message (_id, chat_row_id, from_me, key_id, sender_jid_row_id, status, timestamp, message_type, text_data, starred) VALUES
    (101, 2, 0, 'P1', 4, 13, 20000, 66, 'Onde vamos jantar?', 0),
    (102, 2, 0, 'E1', 4, 13, 21000, 92, 'Aniversário', 0),
    (103, 1, 0, 'PR1', 0, 13, 22000, 23, NULL, 0),
    (104, 1, 0, 'I1', 0, 13, 23000, 24, 'Entre no meu grupo', 0),
    (105, 1, 0, 'T1', 0, 13, 24000, 27, NULL, 0),
    (106, 1, 0, 'U1', 0, 13, 25000, 55, NULL, 0),
    (107, 1, 0, 'U2', 0, 13, 26000, 45, NULL, 0),
    (108, 1, 0, 'D1', 0, 13, 27000, 36, NULL, 0),
    (109, 1, 0, 'L1', 0, 13, 28000, 112, NULL, 0),
    (110, 2, 0, 'H1', 4, 13, 29000, 118, NULL, 0),
    (111, 2, 0, 'W1', 4, 13, 30000, 11, NULL, 0),
    (112, 1, 0, 'R64', 0, 13, 31000, 64, 'texto revogado', 0),
    (113, 2, 0, 'AL', 4, 13, 32000, 99, NULL, 0),
    (114, 2, 0, 'AL1', 4, 13, 32001, 1, NULL, 0),
    (115, 2, 0, 'AL2', 4, 13, 32002, 1, NULL, 0),
    (116, 1, 0, 'V1', 0, 13, 33000, 82, NULL, 0);

INSERT INTO message_poll VALUES (101, 1);
INSERT INTO message_poll_option (message_row_id, option_name, vote_total) VALUES
    (101, 'Pizza', 3), (101, 'Sushi', 1), (101, '  ', 0);
INSERT INTO message_event VALUES (102, 2, 'Aniversário da Ana', 'Traga um bolo', 1700000000, NULL, 'Casa da Ana', NULL, 0, 0);
INSERT INTO message_product VALUES (103, 'Caneca', 'Cerâmica azul', 39900, 'BRL', 'Veja este produto');
INSERT INTO message_group_invite VALUES (104, 'Trilhas');
INSERT INTO message_template VALUES (105, 'Seu pedido saiu para entrega', 'Loja Fictícia');
INSERT INTO message_template_button (message_row_id, text_data) VALUES (105, 'Rastrear'), (105, 'Falar com a loja');
INSERT INTO message_ui_elements (message_row_id, element_type, element_content, button_text, description, footer_text) VALUES
    (106, 1, '{"json":"blob"}', 'Ver opções', 'Escolha um horário', 'Atendimento'),
    (106, 2, NULL, 'Ver opções', NULL, NULL),
    (107, 1, '{"only":"json"}', NULL, NULL, NULL);
INSERT INTO message_ephemeral_setting VALUES (108, 604800);
INSERT INTO message_limit_sharing_setting VALUES (109, 0, 1), (109, 1, 1);
INSERT INTO group_history_metadata VALUES (110, 42);
INSERT INTO message_revoked VALUES (112, 'R64');
INSERT INTO message_album VALUES (113, 2);
INSERT INTO message_association (association_type, parent_message_row_id, child_message_row_id) VALUES
    (1, 113, 115), (1, 113, 114);
INSERT INTO message_add_on (_id, chat_row_id, from_me, key_id, sender_jid_row_id, parent_message_row_id, timestamp, message_add_on_type) VALUES
    (10, 2, 1, 'RA', 0, 113, 32500, 56);
INSERT INTO message_add_on_reaction VALUES (10, '😂', 32500);
