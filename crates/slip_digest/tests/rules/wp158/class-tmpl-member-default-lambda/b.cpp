struct X { static int f(int) { return 1; } static int g(long) { return 2; } int mf(int) { return 1; } int mg(long) { return 2; } };
struct Y { static int f(long) { return 0; } int mf(long) { return 0; } };
template <class T> struct S { int g(int (*p)(T) = [](auto u) { return u.mf(1L); }) { return p(T()); } };
int w() { return S<X>().g(); }
