namespace a {
struct C {
  int n(int);
  int k() { return n(0); }
};
int C::n(int x) { return x; }
}
int u(a::C c, a::C *p) { return c.n(1) + p->n(2) + c.C::n(3) + c.a::C::n(4); }
