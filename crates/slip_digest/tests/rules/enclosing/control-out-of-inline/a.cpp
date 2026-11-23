namespace n { inline namespace v1 { inline int f() { return 1; } } namespace w { } }
int u() { return n::f(); }
