struct X;
int helper(X &);
struct X {
  friend int helper(X &);
private:
  int s = 1;
};
int helper(X &x) { return x.s; }

int caller() { X x; return helper(x); }
