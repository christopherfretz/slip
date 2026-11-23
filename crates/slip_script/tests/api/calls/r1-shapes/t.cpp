namespace N {
int f(int);
struct C {
  int m(int);
  static int s(int);
};
int use(C a, C* p) {
  return f(1) + N::f(2) + (f)(3) + a.m(4) + p->m(5) + C::s(6);
}
}
