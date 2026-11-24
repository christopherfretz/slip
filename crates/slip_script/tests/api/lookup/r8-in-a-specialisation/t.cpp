template <class T>
struct W {
  int a;
  int c;
};
template <>
struct W<int> {
  int b;
};
int use(W<int>& w, W<long>& x) { return w.b + x.a; }
