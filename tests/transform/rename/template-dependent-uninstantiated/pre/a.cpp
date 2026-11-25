namespace a { struct S { int f() { return 1; } }; }
template <class T> int call(T t) { return t.f(); }
int u() { a::S s; return s.f(); }
