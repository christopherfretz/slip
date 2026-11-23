namespace n {
int f(int);
int f(double);
int g(int);
struct S {
  int m;
  int k;
};
template <class T>
int t(T x) {
  return f(x) + f(x) + g(x) + x.m + x.k;
}
}
