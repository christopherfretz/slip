namespace a {
int f(int x) { return x; }
int u() {
  int r = f(1);
  {
    int f = 2;
    r += f;
  }
  return r;
}
}
