namespace n {
template <class T> struct S { int f() { return 1; } };
struct R { int f() { return 3; } };
}
template <class T> int call(T t) { return t.f(); }
int u() { return call(n::S<int>()) + call(n::R()); }
