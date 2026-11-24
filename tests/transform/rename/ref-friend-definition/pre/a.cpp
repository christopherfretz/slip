namespace a {
struct C {
  friend int f(C) { return 1; }
};
int f(C);
}
int u() { a::C c; return f(c) + a::f(c); }
