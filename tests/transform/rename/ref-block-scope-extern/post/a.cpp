namespace a {
int g(int x) { return x; }
int u() {
  int g(int);
  return g(1);
}
}
