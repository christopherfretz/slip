template <class T> int measure(const T &t) {
  if constexpr (requires { t.size(); }) return t.size();
  else return 0;
}

struct Box {
  int size() const { return 3; }
};

int caller() {
  Box b;
  return measure(b);
}
