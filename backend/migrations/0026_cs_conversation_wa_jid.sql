-- CS WhatsApp (Phase 2): map a WhatsApp sender JID to one ongoing conversation.
ALTER TABLE cs_conversation ADD COLUMN wa_jid TEXT;
CREATE INDEX idx_cs_conversation_wa_jid ON cs_conversation (wa_jid);
