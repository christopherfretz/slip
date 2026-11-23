struct Safe {
private:
  int secret = 3;
  friend struct Old;
};
struct New {
  template <class S> static auto probe(S &s, int) -> decltype(s.secret, 1) { return 1; }
  template <class S> static int probe(S &, long) { return 0; }
  static int peek() { Safe s; return probe(s, 0); }
};
