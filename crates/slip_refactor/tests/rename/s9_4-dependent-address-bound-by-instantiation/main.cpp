namespace n {
struct Base { int f() { return 2; } };
template <class T> struct S : Base { int f() { return 1; } };
}
template <class T> int call(T t) { int (T::*p)() = &T::f; return (t.*p)(); }
int u() { return call(n::S<int>()); }
