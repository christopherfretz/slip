namespace app {
struct Val { int n; };
int show(Val) { return 1; }
}
template <class T> int show(T) { return 2; }

int caller(app::Val v, int x) {
  decltype((x)) y = x;
  y = x + 1;
  return show(v) + y + x;
}
