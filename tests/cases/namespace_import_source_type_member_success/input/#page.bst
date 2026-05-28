import @./models

user models.User = models.make_user()
io(user.name)
