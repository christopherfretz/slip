enum Color { red, lime };

template <Color C> struct tag {
  static constexpr int v = C;
};

int pick(Color c) {
  switch (c) {
  case lime:
    return tag<lime>::v;
  default:
    return 0;
  }
}
