struct Old {
  int v;
};
int take(struct Old *) { return 1; }
int take(void *) { return 2; }

int caller() {
  Old o{0};
  return take(&o);
}
