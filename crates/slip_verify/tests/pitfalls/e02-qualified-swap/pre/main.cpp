namespace std {
template <class T> void swap(T &a, T &b) { T t = a; a = b; b = t; }
}

namespace app {
struct Buf { int n; };
void swap(Buf &, Buf &) {}
}

void exchange(app::Buf &a, app::Buf &b, int &i, int &j) {
  using std::swap;
  swap(a, b);
  swap(i, j);
}
