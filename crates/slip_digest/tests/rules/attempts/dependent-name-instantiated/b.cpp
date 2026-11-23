struct S { int f() { return 2; } int g() { return 1; } };
template <class T> int call(T t) { return t.f(); }
int use() { return call(S()); }
