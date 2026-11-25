namespace a {
class C;
int g(const C &);
class C {
  friend int g(const C &);
  int m = 1;
};
int g(const C &c) { return c.m; }
}
int u() { a::C c; return a::g(c); }
