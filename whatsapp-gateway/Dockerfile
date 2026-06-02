# syntax=docker/dockerfile:1
FROM node:20-alpine
WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci --omit=dev
COPY . .

# Baileys persists pairing credentials under ./auth_state -> mount a volume here.
ENV BACKEND_URL=http://backend:8080
CMD ["node", "index.js"]
