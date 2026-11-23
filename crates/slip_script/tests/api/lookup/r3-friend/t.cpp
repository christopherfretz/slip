namespace n {
struct Out;
struct In {
  friend int inf(In);
};
}
struct n::Out {
  friend void outf(Out);
  friend struct Hidden;
};
