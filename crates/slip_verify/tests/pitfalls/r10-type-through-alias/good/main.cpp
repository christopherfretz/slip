struct Gadget {
  int v;
};
using Handle = Gadget;

int read(Handle h) { return h.v; }
