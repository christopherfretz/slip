namespace N {
struct S {
  int v;
};
S operator+(S, S);
int f(int);
struct F {
  int operator()(int);
};
template <class T>
int t(T x) {
  return f(x);
}
template <class T>
int u(T) {
  return f(1);
}
int k(S);
int k2(int);
int use(S a, F fo) {
  int (*pf)(int) = f;
  return k(a + a) + k(S(a)) + k(S{1}) + k2(pf(1)) + k2(fo(2)) + k2(t(3)) +
         u(4) + u(5.0);
}
}
