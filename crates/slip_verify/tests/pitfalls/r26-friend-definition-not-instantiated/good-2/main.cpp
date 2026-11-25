struct X {
  static int f(int) { return 1; }
  static int f(long) { return 2; }
  int mf(int) { return 1; }
  int mg(long) { return 2; }
};
struct D;
template <class T> struct A { friend int fr(X u, D*); };
template <class T> struct B { friend int fr(X u, D*) { return static_cast<T&>(u).mg(1L); } };
struct D : A<X>, B<X> {};
int w() { return fr(X(), (D*)nullptr); }
int main() { return w() == 2 ? 0 : 1; }
