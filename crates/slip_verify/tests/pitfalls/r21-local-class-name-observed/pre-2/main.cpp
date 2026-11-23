const char *f() {
  struct Local {
    const char *who() { return __PRETTY_FUNCTION__; }
  };
  return Local().who();
}
