const char *f() {
  struct Item {
    const char *who() { return __PRETTY_FUNCTION__; }
  };
  return Item().who();
}
