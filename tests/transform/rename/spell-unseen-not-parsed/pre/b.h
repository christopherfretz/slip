namespace a {
int f(int);
inline int u() {
  int g = 2;
  return f(g);
}
}
