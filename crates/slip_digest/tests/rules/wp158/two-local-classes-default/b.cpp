struct X { static int f(int) { return 1; } static int g(long) { return 2; } int mf(int) { return 1; } int mg(long) { return 2; } };
struct Y { static int f(long) { return 0; } int mf(long) { return 0; } };
template <class T> int c(T) { struct A { struct B { int g(int k = T::f(1L)) { return k; } }; }; return typename A::B().g(); }
int w() { return c(X()); }
