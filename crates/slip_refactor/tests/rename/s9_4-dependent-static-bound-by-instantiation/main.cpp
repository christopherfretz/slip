namespace n {
struct Base { static int f() { return 2; } };
template <class T> struct S : Base { static int f() { return 1; } };
}
template <class T> int call() { return T::f(); }
int u() { return call<n::S<int>>(); }
