namespace a { struct S { int f() { return 1; } }; }
template <class T> int call(T t) { return t.f(); }
template <class T> int other(T t) { return t.g(); }
int u() { a::S s; return s.f(); }
