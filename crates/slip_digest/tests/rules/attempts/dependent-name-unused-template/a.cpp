struct S { int f() { return 1; } int g() { return 2; } };
template <class T> int call(T t) { return t.f(); }
