struct Node;

struct Base {
  virtual int get(int k) { return k; }
};

struct Node : Base {
  int v;
  int get(int k) { return this->v + k; }
};
