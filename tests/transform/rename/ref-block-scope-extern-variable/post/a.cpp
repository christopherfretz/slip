namespace a {
int w = 1;
int u() {
  extern int w;
  return w + 1;
}
}
int w2() { return a::w; }
