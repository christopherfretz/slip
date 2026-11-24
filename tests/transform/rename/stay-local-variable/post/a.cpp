namespace a {
int g(int x) { return x; }
int u() {
  int r = g(1);
  {
    int f = 2;
    r += f;
  }
  return r;
}
}
