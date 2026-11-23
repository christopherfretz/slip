enum Color { red, green };

template <Color C> struct tag {
  static constexpr int v = C;
};

int pick(Color c) {
  switch (c) {
  case green:
    return tag<green>::v;
  default:
    return 0;
  }
}
