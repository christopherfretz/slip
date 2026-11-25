struct X { static int f(int) { return 1; } static int g(long) { return 2; } int mf(int) { return 1; } int mg(long) { return 2; } };
struct Y { static int f(long) { return 0; } int mf(long) { return 0; } };
struct D;
template <class T> struct S { friend int fr(X u, D*) { return static_cast<T&>(u).mf(1L); } friend int fr(X u, D*); };
struct D : S<X> {};
int w() { return fr(X(), (D*)nullptr); }
