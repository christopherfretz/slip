namespace a {
int v = 1;
int u() {
  extern int v;
  return v + 1;
}
}
int w2() { return a::v; }
