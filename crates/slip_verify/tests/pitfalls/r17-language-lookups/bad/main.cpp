namespace app {
struct Seq {
  int d[2] = {1, 2};
  int *first() { return d; }
  int *end() { return d + 2; }
};
template <class T> int *begin(T &t) { return t.d + 1; }
template <class T> int *end(T &t) { return t.d + 2; }
}

int sum(app::Seq &s) {
  int n = 0;
  for (int x : s) n += x;
  return n;
}
