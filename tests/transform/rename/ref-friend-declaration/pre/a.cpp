namespace a {
class C;
int f(const C &);
class C {
  friend int f(const C &);
  int m = 1;
};
int f(const C &c) { return c.m; }
}
int u() { a::C c; return a::f(c); }
