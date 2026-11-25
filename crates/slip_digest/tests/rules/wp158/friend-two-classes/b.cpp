struct X { static int f(int) { return 1; } static int g(long) { return 2; } int mf(int) { return 1; } int mg(long) { return 2; } };
struct Y { static int f(long) { return 0; } int mf(long) { return 0; } };
struct D;
template <class T> struct A { friend int fr(X u, D*); };
template <class T> struct B { friend int fr(X u, D*) { return static_cast<T&>(u).mf(1L); } };
struct D : A<X>, B<X> {};
int w() { return fr(X(), (D*)nullptr); }
