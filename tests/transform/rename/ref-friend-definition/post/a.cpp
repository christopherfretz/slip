namespace a {
struct C {
  friend int g(C) { return 1; }
};
int g(C);
}
int u() { a::C c; return g(c) + a::g(c); }
