namespace a { struct S { int f() { return 1; } }; }
template <class T> int call(T t) { return t.f(); }
int u() { return call(a::S{}); }
