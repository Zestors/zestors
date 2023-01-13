[Messages](trait@Message) can be received using an [`Inbox`] with [`Inbox::recv`], [`Inbox::try_recv`] or by using [`futures::stream::Stream`]. Multiple processes of an actor share the same inbox, where messages are delivered to one of the inboxes.

# Closing
When a [Channel] is closed, it is no longer possible to receive new messages, but any messages already in the inbox can still be taken out.
