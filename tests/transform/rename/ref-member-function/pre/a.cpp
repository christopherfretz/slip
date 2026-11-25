namespace a {
struct C {
  int m(int);
  int k() { return m(0); }
};
int C::m(int x) { return x; }
}
int u(a::C c, a::C *p) { return c.m(1) + p->m(2) + c.C::m(3) + c.a::C::m(4); }
