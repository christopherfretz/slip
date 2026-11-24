namespace fresh { template <class T> struct Box { Box(T); };
Box(const char *) -> Box<const char *>; }
fresh::Box b("x");
