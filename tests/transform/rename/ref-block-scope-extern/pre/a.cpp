namespace a {
int f(int x) { return x; }
int u() {
  int f(int);
  return f(1);
}
}
