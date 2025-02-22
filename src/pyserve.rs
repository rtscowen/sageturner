// CONSTRAINT: currently must be in same directory as dockerfile
pub fn get_serve_code(filename: &str) -> String {
    let content = format!(
        r#"
    import sageturner
    from fastapi import FastAPI, Request
    import uvicorn
    model = sageturner.load()
    app = FastAPI()
    @app.post('/ping')
    async def ping():
        if model: 
            return 
        else:
            raise HTTPException(status_code=500, detail="Error")
    @app.post('/invocations')
    async def predict(request: Request):
        body = await request.json()
        response = sageturner.predict(model, request)
        return response
    if __name__ == "__main__":
         config = uvicorn.Config("serve:app", port=8080)
         server = uvicorn.Server(config=config)
         server.run()
    "#);
    content
}
