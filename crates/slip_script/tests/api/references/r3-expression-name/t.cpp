namespace N {
int f(int);
int g(int);
enum E { A };
int v;
int h(int p) {
  auto l = [p] { return p; };
  int (*pg)(int) = &g;
  return f(v) + A + l() + pg(0);
}
}
