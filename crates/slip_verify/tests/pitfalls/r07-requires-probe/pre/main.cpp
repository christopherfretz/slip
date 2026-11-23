template <class T> int measure(const T &) { return 0; }
template <class T>
  requires requires(const T &t) { t.size(); }
int measure(const T &t) { return t.size(); }

struct Box {
  int count() const { return 3; }
};

int caller() {
  Box b;
  return measure(b);
}
