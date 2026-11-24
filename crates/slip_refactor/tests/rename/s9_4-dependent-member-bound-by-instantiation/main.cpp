namespace n {
struct Base { int f() { return 2; } };
template <class T> struct S : Base { int f() { return 1; } };
}
template <class T> int call(T t) { return t.f(); }
int u() { return call(n::S<int>()); }
