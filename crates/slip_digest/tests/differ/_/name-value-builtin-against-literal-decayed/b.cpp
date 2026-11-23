// WP-104 (digest §9), a decayed builtin against a string literal of its text, as an argument
void take(const char *);
void f() { take("f"); }
